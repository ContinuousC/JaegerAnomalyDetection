/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use jaeger_anomaly_detection::{ImmediateInterval, ReferenceInterval};
use prometheus_remote_write::{Label, TimeSeries, WriteRequest};

use crate::{
    config::ConfigName,
    jaeger::{Bool, TagValue},
    processor::trace::MetricArgs,
};

#[derive(Default)]
pub struct Metrics(BTreeMap<BTreeMap<String, String>, Vec<prometheus_remote_write::Sample>>);

#[derive(Default)]
pub struct Labels {
    pub q: Option<String>,
    pub le: Option<String>,
    pub immediate: Option<ImmediateInterval>,
    pub reference: Option<ReferenceInterval>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.values().map(|samples| samples.len()).sum()
    }

    pub fn split_off(&mut self, max: usize) -> Self {
        // TODO: use extract_if when stabilized, or find some other
        // more efficient means of doing this
        let metrics = std::mem::take(&mut self.0);
        let mut r = BTreeMap::new();
        metrics.into_iter().enumerate().for_each(|(i, (k, v))| {
            if i < max {
                r.insert(k, v);
            } else {
                self.0.insert(k, v);
            }
        });
        Self(r)
    }

    pub fn insert(&mut self, labels: BTreeMap<String, String>, t: DateTime<Utc>, value: f64) {
        self.0
            .entry(labels)
            .or_default()
            .push(prometheus_remote_write::Sample {
                value,
                timestamp: t.timestamp_millis(),
            })
    }

    pub fn into_write_request(self) -> WriteRequest {
        WriteRequest {
            timeseries: self
                .0
                .into_iter()
                .map(|(labels, samples)| TimeSeries {
                    labels: labels
                        .into_iter()
                        .map(|(name, value)| Label { name, value })
                        .collect(),
                    samples,
                })
                .collect(),
        }
    }

    pub(crate) fn add_metric(
        &mut self,
        metric: MetricArgs<'_>,
        config_name: &ConfigName,
        t: DateTime<Utc>,
        value: f64,
    ) {
        let mut labels = BTreeMap::new();
        labels.insert(String::from("__name__"), metric.metric_name);
        labels.insert(String::from("metric_type"), metric.metric_type.to_string());
        labels.insert(String::from("config"), config_name.to_string());
        for (name, value) in metric.key {
            let label = name.label().into_string();
            let value = match value {
                TagValue::String(s) => s.to_string(),
                TagValue::Int64(v) => format!("{}", v.0),
                TagValue::Bool(Bool::True) => String::from("true"),
                TagValue::Bool(Bool::False) => String::from("false"),
            };
            labels.insert(label, value);
        }
        if let Some(interval) = metric.labels.immediate {
            labels.insert(String::from("immediate"), interval.to_string());
        }
        if let Some(interval) = metric.labels.reference {
            labels.insert(String::from("reference"), interval.to_string());
        }
        if let Some(le) = metric.labels.le {
            labels.insert(String::from("le"), le);
        }
        if let Some(q) = metric.labels.q {
            labels.insert(String::from("quantile"), q);
        }
        self.insert(labels, t, value);
    }
}
