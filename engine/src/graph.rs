/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{collections::BTreeMap, fmt::Display};

use actix_web::{web::Query, HttpResponse};
use apistos::{api_operation, ApiComponent};
use chrono::{DateTime, Utc};
use prometheus_api::{GenericQueryResponse, Matrix, QueryResult, RangeQuery, RangeQueryParams};
use prometheus_core::{LabelName, MetricName};
use prometheus_expr::PromDuration;
use reqwest::Client;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::instrument;

use jaeger_anomaly_detection::{WelfordExprs, WelfordParams};

#[derive(Deserialize, JsonSchema, ApiComponent, Debug)]
pub struct Params {
    r#type: GraphType,
    operation: Option<String>,
    service: Option<String>,
    #[serde(default = "default_duration")]
    duration: PromDuration,
    #[serde(default = "default_q")]
    q: f64,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    #[serde(default = "default_interval")]
    interval: PromDuration,
}

const fn default_duration() -> PromDuration {
    PromDuration::Minutes(5)
}

const fn default_q() -> f64 {
    0.99
}

const fn default_interval() -> PromDuration {
    PromDuration::Days(1)
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
enum GraphType {
    Duration,
    Busy,
    CallRate,
    ErrorRate,
}

impl GraphType {
    fn metric(&self) -> (MetricName, f64) {
        match self {
            GraphType::Duration => (MetricName::new_static("duration"), 1e6),
            GraphType::Busy => (MetricName::new_static("busy"), 1e9),
            GraphType::CallRate => (MetricName::new_static("call_rate"), 1e0),
            GraphType::ErrorRate => (MetricName::new_static("error_rate"), 1e0),
        }
    }
}

impl Display for GraphType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphType::Duration => write!(f, "duration"),
            GraphType::Busy => write!(f, "busy"),
            GraphType::CallRate => write!(f, "call_rate"),
            GraphType::ErrorRate => write!(f, "error_rate"),
        }
    }
}

#[api_operation(summary = "Show example graph")]
#[instrument]
pub async fn get_example_graph(params: Query<Params>) -> HttpResponse {
    let Params {
        r#type,
        operation,
        service,
        duration,
        q,
        from,
        to,
        interval,
    } = params.into_inner();

    let (metric, factor) = r#type.metric();

    let exprs = WelfordExprs::new(&WelfordParams {
        metric: metric.clone(),
        labels: operation
            .as_ref()
            .map(|value| (LabelName::new("operation_name").unwrap(), value.clone()))
            .into_iter()
            .chain(
                service
                    .as_ref()
                    .map(|value| (LabelName::new("service_name").unwrap(), value.clone())),
            )
            .collect(),
        group_by: None,
        duration,
        q,
        labels_selectors: BTreeMap::new(),
    });

    let n = 200;
    let end = to.unwrap_or_else(Utc::now);
    let start = from.unwrap_or_else(|| end - interval.to_time_delta());
    let step = (end - start) / n;

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    let url = "https://tenant-mdp.continuousc.contc/api/prom/api/v1/query_range";
    let params = RangeQueryParams {
        start,
        end,
        step: (step.num_milliseconds() as f64) / 1e3,
    };

    let count = prom_query(&client, url, &params, &exprs.count.to_string()).await;
    let mean = prom_query(&client, url, &params, &exprs.mean.to_string()).await;
    let confidence_interval = prom_query(
        &client,
        url,
        &params,
        &exprs.confidence_interval.to_string(),
    )
    .await;

    let options = serde_json::to_string(&json!({
        "title": {
            "text": format!("{type} for service {} / operation {}",
                            service.as_deref().unwrap_or("-"),
                            operation.as_deref().unwrap_or("-"))
        },
        "tooltip": {},
        "legend": {
            "data": [
                "count",
                r#type,
                {
                    "name": "confidence interval",
                    "itemStyle": {
                        "color": "#ccc"
                    }
                }
            ]
        },
        "xAxis": {
            "type": "time"
        },
        "yAxis": [
            {
                "name": r#type
            },
            {
                "name": "count"
            }
        ],
        "tooltip": {
            "trigger": "axis"
        },
        "series": [
            {
                "name": "count",
                "type": "line",
                "data": count.iter().collect::<Vec<_>>(),
                "yAxisIndex": 1
            },
            {
                "name": "confidence interval lower bound",
                "type": "line",
                "data": mean.iter().map(|(t, mean)| {
                    let ci = confidence_interval.get(t).copied().unwrap_or(f64::NAN);
                    let low = (mean - ci).max(0.0);
                    (t, low / factor)
                }).collect::<Vec<_>>(),
                "lineStyle": {
                    "opacity": 0
                },
                "stack": "confidence-band",
                "symbol": "none"
            },
            {
                "name": "confidence interval",
                "type": "line",
                "data": mean.iter().map(|(t,mean)| {
                    let ci = confidence_interval.get(t).copied().unwrap_or(f64::NAN);
                    let low = (mean - ci).min(0.0);
                    let high = mean + ci;
                    Some((t, (high - low) / factor))
                }).collect::<Vec<_>>(),
                "lineStyle": {
                    "opacity": 0
                },
                "areaStyle": {
                    "color": "#ccc"
                },
                "stack": "confidence-band",
                "symbol": "none"
            },
            {
                "name": r#type,
                "type": "line",
                "data": mean.iter().map(|(t,v)| (t, *v / factor)).collect::<Vec<_>>()
            }
        ]
    }))
    .unwrap();

    let doc = format!(
        r#"
<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8" />
<title>Example graph: {type}</title>
<script src="https://cdn.jsdelivr.net/npm/echarts@5.5.0/dist/echarts.min.js"></script>
</head>
<body>
<div id="graph" style="position: absolute; top: 0; bottom: 0; left: 0; right: 0;"></div>
<script type="text/javascript">
var myChart = echarts.init(document.getElementById('graph'));
var option = {options};
myChart.setOption(option);
</script>
</body>
</html>
"#
    );
    HttpResponse::Ok().content_type("text/html").body(doc)
}

async fn prom_query(
    client: &Client,
    url: &str,
    params: &RangeQueryParams,
    query: &str,
) -> BTreeMap<String, f64> {
    let res = client
        .post(url)
        .form(&RangeQuery {
            query,
            params: params.clone(),
        })
        .send()
        .await
        .unwrap();

    if !res.status().is_success() {
        let msg = res.text().await.unwrap();
        panic!("query failed: {msg}");
    }

    let data = res.json::<GenericQueryResponse>().await.unwrap();

    let row = match data.into_result().unwrap().data {
        QueryResult::Matrix(rows) => match rows.into_iter().next() {
            Some(row) => row,
            None => return Default::default(),
        },
        _ => panic!(),
    };

    match row.value {
        Matrix::Values(values) => values
            .into_iter()
            .map(|v| (v.timestamp.to_rfc3339(), v.value.0))
            .collect(),
        _ => panic!(),
    }
}
