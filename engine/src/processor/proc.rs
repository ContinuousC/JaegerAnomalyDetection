/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{collections::BTreeMap, path::Path, sync::Arc};

use chrono::{DateTime, TimeDelta, Utc};
use reqwest::header::{HeaderMap, HeaderValue};
use tap::Pipe;
use tokio::task::JoinHandle;
use url::Url;

use crate::{
    config::Config,
    error::{Error, Result},
    jaeger::Span,
    metrics::Metrics,
    opensearch::{
        EsCreatePitQuery, EsCreatePitResponse, EsDeletePitRequest, EsDeletePitResponse, EsPit,
        EsRel, EsResponse, EsSearchRequest, EsSearchResponse, EsSortField, EsSortOpts, EsSortOrder,
    },
    state::State,
    Args, BATCH_SIZE, CHUNK_SIZE, INDEX, KEEP_ALIVE, MAX_SPANS,
};

use super::trace::TraceProcessor;

#[derive(Debug)]
pub struct Processor {
    processor: JoinHandle<Result<()>>,
    term_sender: tokio::sync::oneshot::Sender<()>,
    config_sender: tokio::sync::watch::Sender<Arc<Config>>,
}

impl Processor {
    pub async fn new(args: &Args) -> Result<Self> {
        let ca = reqwest::tls::Certificate::from_pem_bundle(
            &tokio::fs::read(&args.opensearch_ca)
                .await
                .map_err(|e| Error::ReadFile(args.opensearch_ca.clone(), e))?,
        )
        .map_err(|e| Error::LoadCa(args.opensearch_ca.clone(), e))?;

        let id = reqwest::tls::Identity::from_pkcs8_pem(
            &tokio::fs::read(&args.opensearch_cert)
                .await
                .map_err(|e| Error::ReadFile(args.opensearch_cert.clone(), e))?,
            &tokio::fs::read(&args.opensearch_key)
                .await
                .map_err(|e| Error::ReadFile(args.opensearch_key.clone(), e))?,
        )
        .map_err(|e| {
            Error::LoadCert(args.opensearch_cert.clone(), args.opensearch_key.clone(), e)
        })?;

        let esclient = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .pipe(|client| {
                ca.iter().fold(client, |client, cert| {
                    client.add_root_certificate(cert.clone())
                })
            })
            // .danger_accept_invalid_hostnames(true)
            .identity(id)
            .build()
            .map_err(Error::Elastic)?;

        let promclient = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .pipe(|client| {
                ca.into_iter()
                    .fold(client, |client, cert| client.add_root_certificate(cert))
            })
            // .danger_accept_invalid_hostnames(true)
            .default_headers({
                let mut headers = HeaderMap::new();
                if let Some(tenant) = &args.prometheus_tenant {
                    headers.insert(
                        "X-Scope-OrgID",
                        HeaderValue::try_from(tenant).map_err(Error::InvalidPrometheusTenant)?,
                    );
                }
                headers
            })
            .build()
            .map_err(Error::Prometheus)?;

        let (mut config, state, last) = if args.state.exists() {
            let data = tokio::fs::read(&args.state)
                .await
                .map_err(Error::ReadState)?;
            let state = ciborium::from_reader::<State, _>(data.as_slice())
                .map_err(Error::DeserializeState)?;
            (state.config, Some(state.state), Some(state.last))
        } else {
            (Config::default(), None, None)
        };

        let orig_trace_config = std::mem::take(&mut config.trace);

        let (term_sender, mut term_receiver) = tokio::sync::oneshot::channel::<()>();
        let (config_sender, mut config_receiver) = tokio::sync::watch::channel(Arc::new(config));

        let args = args.clone();
        let processor = tokio::spawn(async move {
            let mut config = config_receiver.borrow_and_update().clone();

            let mut interval = tokio::time::interval(
                config
                    .query_interval
                    .to_time_delta()
                    .to_std()
                    .map_err(Error::DateTimeBounds)?,
            );

            let mut from = Utc::now() - config.max_history.to_time_delta();
            if let Some(last) = last {
                from = from.max(last);
            }

            let mut processor = state.map_or_else(
                || TraceProcessor::new(&config.trace),
                |state| {
                    let proc = TraceProcessor::load(from, state, &orig_trace_config);
                    proc.update(from, &config.trace)
                },
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let to = Utc::now() - config.delay.to_time_delta();

                        log::info!("processing traces from {from} to {to}...");
                        if let Err(e) = process_traces(
                            &args,
                            &config,
                            &esclient,
                            &promclient,
                            from,
                            to,
                            &mut processor,
                        )
                        .await
                        {
                            log::error!("{e}");
                        } else {
                            from = to;
                        }

                        write_state(&processor, &config, to, &args.state).await;
                    }
                    _ = config_receiver.changed() => {
                        let new = config_receiver.borrow_and_update().clone();
                        if config == new {
                            log::info!("config unchanged -- skipping update");
                             continue;
                        }
                        log::info!("updating config");
                        config = new;
                        interval =
                            tokio::time::interval(config.query_interval.to_time_delta().to_std().map_err(Error::DateTimeBounds)?);
                        processor = processor.update(from, &config.trace);
                        write_state(&processor, &config, from, &args.state).await;
                    }
                    _ = &mut term_receiver => {
                        break;
                    }
                }
            }

            Ok(())
        });

        Ok(Self {
            processor,
            term_sender,
            config_sender,
        })
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config_sender.borrow().clone()
    }

    pub fn update_config(&self, config: Config) {
        self.config_sender.send(Arc::new(config)).unwrap();
    }

    pub async fn shutdown(self) -> Result<()> {
        self.term_sender.send(()).unwrap();
        self.processor.await.map_err(Error::JoinProcessor)?
    }
}

async fn write_state(
    processor: &TraceProcessor,
    config: &Config,
    last: DateTime<Utc>,
    path: &Path,
) {
    let state = processor.save();
    let mut data = Vec::new();
    ciborium::into_writer(
        &State {
            config: (*config).clone(),
            last,
            state,
        },
        &mut data,
    )
    .unwrap();

    if let Err(e) = tokio::fs::write(path, data)
        .await
        .map_err(Error::WriteState)
    {
        log::warn!("{e}");
    } else {
        log::info!("state saved")
    }
}

async fn process_traces(
    args: &Args,
    config: &Config,
    esclient: &reqwest::Client,
    promclient: &reqwest::Client,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    processor: &mut TraceProcessor,
) -> Result<()> {
    let sample_interval = config.query_interval.to_time_delta();
    let mut next_sample = from + sample_interval;
    let mut metrics = Metrics::new();
    let min_timestamp = Utc::now() - TimeDelta::hours(1);

    struct Handler<'a> {
        args: &'a Args,
        promclient: &'a reqwest::Client,
        sample_interval: TimeDelta,
        next_sample: &'a mut DateTime<Utc>,
        metrics: &'a mut Metrics,
        processor: &'a mut TraceProcessor,
        min_timestamp: DateTime<Utc>,
    }

    impl TraceHandler for Handler<'_> {
        async fn handle(&mut self, root: &Span, spans: &[Span]) -> Result<()> {
            let t = DateTime::from_timestamp_micros(root.start_time).ok_or(Error::DateTime)?;
            while *self.next_sample < t {
                if *self.next_sample >= self.min_timestamp {
                    self.processor
                        .sample(*self.next_sample, |metric_args, config_name, value| {
                            self.metrics.add_metric(
                                metric_args,
                                config_name,
                                *self.next_sample,
                                value,
                            );
                        });
                }
                *self.next_sample += self.sample_interval;

                while self.metrics.len() > self.args.metrics_per_request {
                    if let Err(e) = write_metrics(
                        self.metrics.split_off(self.args.metrics_per_request),
                        self.promclient,
                        &self.args.prometheus_url,
                    )
                    .await
                    {
                        log::warn!("{e}");
                    }
                }
            }

            self.processor.insert(t, spans);
            Ok(())
        }
    }

    for_traces(
        args,
        esclient,
        from,
        to,
        Handler {
            args,
            promclient,
            sample_interval,
            next_sample: &mut next_sample,
            metrics: &mut metrics,
            processor,
            min_timestamp,
        },
    )
    .await?;

    while next_sample < to {
        processor.sample(next_sample, |metric_args, config_name, value| {
            metrics.add_metric(metric_args, config_name, next_sample, value);
        });
        next_sample += sample_interval;

        while metrics.len() > args.metrics_per_request {
            if let Err(e) = write_metrics(
                metrics.split_off(args.metrics_per_request),
                promclient,
                &args.prometheus_url,
            )
            .await
            {
                log::warn!("{e}");
            }
        }
    }

    while !metrics.is_empty() {
        if let Err(e) = write_metrics(
            metrics.split_off(args.metrics_per_request),
            promclient,
            &args.prometheus_url,
        )
        .await
        {
            log::warn!("{e}");
        }
    }

    processor.cleanup(to - TimeDelta::days(30));

    Ok(())
}

// struct ShowLabels<'a>(
//     &'a BTreeMap<&'a KeyName, TagValue>,
//     &'a ConfigName,
//     &'a Labels,
// );

// impl Display for ShowLabels<'_> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "config = {}", self.1)?;
//         for (label, value) in self.0 {
//             write!(f, ", ")?;
//             match label {
//                 KeyName::OperationName => write!(f, "operation_name")?,
//                 KeyName::ServiceName => write!(f, "service_name")?,
//                 KeyName::ProcessTag(tag) => write!(f, "{tag}")?,
//                 KeyName::SpanTag(tag) => write!(f, "{tag}")?,
//                 KeyName::Duration => write!(f, "duration")?,
//             }
//             write!(f, "=")?;
//             match value {
//                 TagValue::String(s) => write!(f, "\"{s}\"")?,
//                 TagValue::Int64(v) => write!(f, "\"{}\"", v.0)?,
//                 TagValue::Bool(Bool::True) => write!(f, "\"true\"")?,
//                 TagValue::Bool(Bool::False) => write!(f, "\"false\"")?,
//             }
//         }
//         if let Some(le) = &self.2.le {
//             write!(f, ", le = {le}")?;
//         }
//         if let Some(q) = &self.2.q {
//             write!(f, ", q = {q}")?;
//         }
//         Ok(())
//     }
// }

async fn write_metrics(
    metrics: Metrics,
    promclient: &reqwest::Client,
    prom_url: &Url,
) -> Result<()> {
    log::info!("writing {} metrics", metrics.len());
    let req = metrics
        .into_write_request()
        .build_http_request(prom_url, "ContinuousC")
        .map_err(Error::BuildPromRequest)?;
    let res = promclient
        .execute(reqwest::Request::try_from(req).map_err(Error::Prometheus)?)
        .await
        //.and_then(|r| r.error_for_status())
        .map_err(Error::Prometheus)?
        .text()
        .await
        .map_err(Error::Prometheus)?;
    res.is_empty()
        .then_some(())
        .ok_or_else(|| Error::PromRes(res))
}

trait TraceHandler {
    async fn handle(&mut self, root: &Span, spans: &[Span]) -> Result<()>;
}

async fn for_traces<T: TraceHandler>(
    args: &Args,
    client: &reqwest::Client,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    mut handler: T,
) -> Result<()> {
    let mut pit_id = client
        .post(
            args.opensearch_url
                .join(&format!("{}/_search/point_in_time", INDEX))
                .map_err(Error::Url)?,
        )
        .query(&EsCreatePitQuery {
            keep_alive: KEEP_ALIVE,
            allow_partial_pit_creation: false,
        })
        .pipe(|c| match &args.opensearch_user {
            Some(username) => c.basic_auth(username, args.opensearch_password.as_ref()),
            None => c,
        })
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(Error::Elastic)?
        .json::<EsResponse<EsCreatePitResponse>>()
        .await
        .map_err(Error::Elastic)?
        .into_result()?
        .pit_id;

    let mut last = None;

    let query = serde_json::json!({
        "bool": {
            "must": [
                {
                    "range": {
                        "startTime": {
                            "gte": from.timestamp_micros(),
                            "lt": to.timestamp_micros()
                        }
                    }
                },
                find_root_spans()
            ]
        }
    });

    let res = async {
        loop {
            let res = client
                .post(args.opensearch_url.join("_search").map_err(Error::Url)?)
                .json(&EsSearchRequest {
                    query: &query,
                    size: BATCH_SIZE,
                    pit: Some(EsPit {
                        id: pit_id.clone(),
                        keep_alive: KEEP_ALIVE,
                    }),
                    sort: Some(vec![EsSortField {
                        field: String::from("startTime"),
                        opts: EsSortOpts {
                            order: EsSortOrder::Asc,
                        },
                    }]),
                    search_after: last,
                })
                .pipe(|c| match &args.opensearch_user {
                    Some(username) => c.basic_auth(username, args.opensearch_password.as_ref()),
                    None => c,
                })
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(Error::Elastic)?
                .json::<EsResponse<EsSearchResponse<Span, (i64,)>>>()
                .await
                .map_err(Error::Elastic)?
                .into_result()?;

            pit_id = res.pit_id.ok_or(Error::ElasticMissingPitId)?;

            if res.hits.hits.is_empty() {
                break;
            }

            last = res.hits.hits.last().unwrap().sort;

            for roots in res.hits.hits.chunks(CHUNK_SIZE) {
                let res = client
                    .post(args.opensearch_url.join("_search").map_err(Error::Url)?)
                    .json(&EsSearchRequest::<_, ()> {
                        query: serde_json::json!({
                            "terms": {
                                "traceID": roots
                                    .iter()
                                    .map(|root| &root.source.trace_id)
                                    .collect::<Vec<_>>()
                            }
                        }),
                        size: MAX_SPANS,
                        pit: Some(EsPit {
                            id: pit_id.clone(),
                            keep_alive: KEEP_ALIVE,
                        }),
                        sort: Some(vec![EsSortField {
                            field: String::from("startTime"),
                            opts: EsSortOpts {
                                order: EsSortOrder::Asc,
                            },
                        }]),
                        search_after: None,
                    })
                    .pipe(|c| match &args.opensearch_user {
                        Some(username) => c.basic_auth(username, args.opensearch_password.as_ref()),
                        None => c,
                    })
                    .send()
                    .await
                    .and_then(|r| r.error_for_status())
                    .map_err(Error::Elastic)?
                    .json::<EsResponse<EsSearchResponse<Span, (i64,)>>>()
                    .await
                    .map_err(Error::Elastic)?
                    .into_result()?;

                assert!(res.hits.total.relation == EsRel::Eq);
                pit_id = res.pit_id.ok_or(Error::ElasticMissingPitId)?;

                let traces =
                    res.hits
                        .hits
                        .into_iter()
                        .fold(BTreeMap::<_, Vec<_>>::new(), |mut map, hit| {
                            map.entry(hit.source.trace_id.clone())
                                .or_default()
                                .push(hit.source);
                            map
                        });

                for root in roots {
                    if let Some(spans) = traces.get(&root.source.trace_id) {
                        handler.handle(&root.source, spans).await?;
                    } else {
                        eprintln!("warning: no spans found for {}", root.source.trace_id);
                    }
                }
            }
        }

        Ok(())
    }
    .await;

    client
        .delete(
            args.opensearch_url
                .join("_search/point_in_time")
                .map_err(Error::Url)?,
        )
        .json(&EsDeletePitRequest { pit_id })
        .pipe(|c| match &args.opensearch_user {
            Some(username) => c.basic_auth(username, args.opensearch_password.as_ref()),
            None => c,
        })
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(Error::Elastic)?
        .json::<EsResponse<EsDeletePitResponse>>()
        .await
        .map_err(Error::Elastic)?
        .into_result()?;

    match res {
        Ok(()) => {
            log::info!("finished processing traces");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

// async fn get_spans(args: &Args, client: &reqwest::Client, trace_id: &TraceId) -> Result<Vec<Span>> {
//     let res = client
//         .post(args.es_url.join("_search").unwrap())
//         .json(&EsSearchRequest::<_, ()> {
//             query: serde_json::json!({
//                 "term": {
//                     "traceID": trace_id
//                 }
//             }),
//             size: MAX_SPANS,
//             pit: None,
//             sort: Some(vec![EsSortField {
//                 field: String::from("startTime"),
//                 opts: EsSortOpts {
//                     order: EsSortOrder::Asc,
//                 },
//             }]),
//             search_after: None,
//         })
//         .basic_auth(&args.user, Some(&args.password))
//         .send()
//         .await
//         .and_then(|r| r.error_for_status())
//         .map_err(Error::Elastic)?
//         .json::<EsResponse<EsSearchResponse<Span, (i64,)>>>()
//         .await
//         .map_err(Error::Elastic)?
//         .into_result()?;

//     assert!(res.hits.total.relation == EsRel::Eq);

//     Ok(res
//         .hits
//         .hits
//         .into_iter()
//         .map(|hit| hit.source)
//         .collect::<Vec<_>>())
// }

fn find_root_spans() -> serde_json::Value {
    serde_json::json!({
        "bool": {
            "must_not": {
                "nested": {
                    "path": "references",
                    "query": {
                        "term": {
                            "references.refType": {
                                "value": "CHILD_OF"
                            }
                        }
                    }
                }
            }
        }
    })
}
