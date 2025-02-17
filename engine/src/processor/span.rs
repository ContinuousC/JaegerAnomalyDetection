/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    config::{MetricName, SpanKey},
    jaeger::{Span, TagValue},
};

use super::{
    metric::{MetricConfig, MetricProcessor, MetricState},
    trace::MetricArgs,
};

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
pub struct SpanConfig {
    pub key: BTreeSet<SpanKey>,
    pub metrics: BTreeMap<MetricName, MetricConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SpanState {
    groups: BTreeMap<BTreeMap<SpanKey, TagValue>, MetricsState>,
}

#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum MetricsState {
    V0(BTreeMap<MetricName, MetricState>),
    V1(MetricsStateV1),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsStateV1 {
    last_seen: DateTime<Utc>,
    metrics: BTreeMap<MetricName, MetricState>,
}

// Manual 'untagged' deserialization impl while
// https://github.com/serde-rs/serde/pull/2781 is open.

impl<'de> Deserialize<'de> for MetricsState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = ciborium::Value::deserialize(deserializer)?;
        value
            .deserialized()
            .map(MetricsState::V0)
            .or_else(|_| value.deserialized().map(MetricsState::V1))
            .map_err(<D::Error as serde::de::Error>::custom)
    }
}

pub struct SpanProcessor {
    config: SpanConfig,
    groups: BTreeMap<BTreeMap<SpanKey, TagValue>, MetricsProcessor>,
}

pub struct MetricsProcessor {
    last_seen: DateTime<Utc>,
    metrics: BTreeMap<MetricName, MetricProcessor>,
}

impl SpanProcessor {
    pub fn new(config: &SpanConfig) -> Self {
        Self {
            config: config.clone(),
            groups: BTreeMap::new(),
        }
    }

    pub fn update(self, t: DateTime<Utc>, config: &SpanConfig) -> SpanProcessor {
        SpanProcessor {
            config: config.clone(),
            groups: if self.config.key == config.key {
                self.groups
                    .into_iter()
                    .map(|(key, mut metrics)| {
                        metrics.metrics = config
                            .metrics
                            .iter()
                            .map(|(name, config)| {
                                if let Some(proc) = metrics.metrics.remove(name) {
                                    (name.clone(), proc.update(t, config))
                                } else {
                                    (name.clone(), MetricProcessor::new(t, config))
                                }
                            })
                            .collect();
                        (key, metrics)
                    })
                    .collect()
            } else {
                BTreeMap::new()
            },
        }
    }

    pub fn load(t: DateTime<Utc>, state: SpanState, config: &SpanConfig) -> Self {
        Self {
            config: config.clone(),
            groups: state
                .groups
                .into_iter()
                .map(|(key, proc)| {
                    let (last_seen, mut metrics) = match proc {
                        MetricsState::V1(MetricsStateV1 { last_seen, metrics }) => {
                            (last_seen, metrics)
                        }
                        MetricsState::V0(metrics) => (t - TimeDelta::days(29), metrics),
                    };
                    let metrics = config
                        .metrics
                        .iter()
                        .map(|(name, config)| {
                            let proc = metrics.remove(name).map_or_else(
                                || MetricProcessor::new(t, config),
                                |state| MetricProcessor::load(t, state, config),
                            );
                            (name.clone(), proc)
                        })
                        .collect();
                    (key, MetricsProcessor { last_seen, metrics })
                })
                .collect(),
        }
    }

    pub fn save(&self) -> SpanState {
        SpanState {
            groups: self
                .groups
                .iter()
                .map(|(key, proc)| {
                    let key = key
                        .iter()
                        .map(|(name, value)| ((*name).clone(), value.clone()))
                        .collect();
                    let metrics = proc
                        .metrics
                        .iter()
                        .map(|(name, proc)| ((*name).clone(), proc.save()))
                        .collect();
                    (
                        key,
                        MetricsState::V1(MetricsStateV1 {
                            last_seen: proc.last_seen,
                            metrics,
                        }),
                    )
                })
                .collect(),
        }
    }

    pub fn insert(
        &mut self,
        t: DateTime<Utc>,
        span: &Span,
        parent: Option<&Span>,
        children: &[&Span],
    ) {
        let key = self
            .config
            .key
            .iter()
            .filter_map(|key| Some((key.clone(), key.get(span, parent)?.to_owned())))
            .collect();
        self.groups
            .entry(key)
            .or_insert_with(|| {
                let metrics = self
                    .config
                    .metrics
                    .iter()
                    .map(|(name, config)| (name.clone(), MetricProcessor::new(t, config)))
                    .collect();
                MetricsProcessor {
                    last_seen: t,
                    metrics,
                }
            })
            .metrics
            .values_mut()
            .for_each(|proc| {
                proc.insert(t, span, parent, children);
            });
    }

    pub fn sample<F: FnMut(MetricArgs<'_>, f64)>(&mut self, t: DateTime<Utc>, mut metric: F) {
        self.groups.iter_mut().for_each(|(key, metrics)| {
            metrics.metrics.iter_mut().for_each(|(name, proc)| {
                proc.sample(
                    t,
                    |super::metric::MetricArgs {
                         metric_suffix,
                         metric_type,
                         labels,
                     },
                     value| {
                        let name = metric_suffix
                            .map_or_else(|| name.to_string(), |suffix| format!("{name}_{suffix}"));
                        metric(
                            MetricArgs {
                                metric_name: format!("trace_{name}"),
                                metric_type,
                                labels,
                                key,
                            },
                            value,
                        )
                    },
                );
            });
        });
    }

    pub fn cleanup(&mut self, t: DateTime<Utc>) {
        self.groups.retain(|_, proc| proc.last_seen >= t);
    }
}
