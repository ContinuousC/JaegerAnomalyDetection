/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use jaeger_anomaly_detection::{ImmediateInterval, ReferenceInterval};
use ordered_float::NotNan;
use rustc_apfloat::{ieee::Quad, Float};
use serde::{Deserialize, Serialize};

use crate::{
    accum::Accum,
    metrics::Labels,
    welford::{from_f64, to_f64, Welford},
    window::Window,
};

use super::metric::MetricArgs;

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct AnomalyScoreConfig {
    reference_intervals: BTreeSet<ReferenceInterval>,
    immediate_intervals: BTreeSet<ImmediateInterval>,
    #[schemars(with = "f64")]
    offset: ordered_float::NotNan<f64>,
    #[schemars(with = "f64")]
    q: ordered_float::NotNan<f64>,
}

pub type AnomalyScoreState = AnomalyScoreProcessor;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnomalyScoreProcessor {
    welford: Welford<Quad>,
    config: AnomalyScoreConfig,
    immediate: BTreeMap<ImmediateInterval, Window<Welford<Quad>>>,
    reference: BTreeMap<ReferenceInterval, Window<Welford<Quad>>>,
}

impl AnomalyScoreProcessor {
    pub fn new(t: DateTime<Utc>, config: &AnomalyScoreConfig) -> Self {
        Self {
            welford: Welford::default(),
            config: config.clone(),
            immediate: config
                .immediate_intervals
                .iter()
                .map(|interval| (*interval, Window::new(t, &interval.window_config())))
                .collect(),
            reference: config
                .reference_intervals
                .iter()
                .map(|interval| (*interval, Window::new(t, &interval.window_config())))
                .collect(),
        }
    }

    pub fn update(&self, t: DateTime<Utc>, config: &AnomalyScoreConfig) -> Self {
        Self {
            welford: self.welford.clone(),
            config: config.clone(),
            immediate: config
                .immediate_intervals
                .iter()
                .map(|interval| {
                    self.immediate
                        .get(interval)
                        .filter(|window| window.compatible_with(&interval.window_config()))
                        .map_or_else(
                            || {
                                (
                                    *interval,
                                    Window::new_init(
                                        t,
                                        |_| self.welford.clone(),
                                        &interval.window_config(),
                                    ),
                                )
                            },
                            |window| (*interval, window.clone()),
                        )
                })
                .collect(),
            reference: config
                .reference_intervals
                .iter()
                .map(|interval| {
                    self.reference
                        .get(interval)
                        .filter(|window| window.compatible_with(&interval.window_config()))
                        .map_or_else(
                            || {
                                (
                                    *interval,
                                    Window::new_init(
                                        t,
                                        |_| self.welford.clone(),
                                        &interval.window_config(),
                                    ),
                                )
                            },
                            |window| (*interval, window.clone()),
                        )
                })
                .collect(),
        }
    }

    pub fn load(t: DateTime<Utc>, state: AnomalyScoreState, config: &AnomalyScoreConfig) -> Self {
        state.update(t, config)
    }

    pub fn save(&self) -> AnomalyScoreState {
        self.clone()
    }

    pub fn insert(&mut self, t: DateTime<Utc>, value: f64) {
        let prev = self.welford.clone();
        self.welford.insert(value);
        let value = |end: DateTime<Utc>| {
            if t >= end {
                self.welford.clone()
            } else {
                prev.clone()
            }
        };
        self.immediate.values_mut().for_each(|window| {
            let bin_width = window.bin_width();
            window.advance_init(t, |s| value(s + bin_width));
        });
        self.reference.values_mut().for_each(|window| {
            let bin_width = window.bin_width();
            window.advance_init(t, |s| value(s + bin_width));
        });
    }

    pub fn sample<F: FnMut(MetricArgs, f64)>(&self, mut metric: F) {
        let q = self.config.q.into_inner();
        let offset = from_f64(self.config.offset.into_inner());

        let immediate = self
            .immediate
            .iter()
            .map(|(immediate_interval, immediate)| {
                metric(
                    MetricArgs {
                        metric_suffix: Some("count"),
                        metric_type: "anomaly_score",
                        labels: Labels {
                            immediate: Some(*immediate_interval),
                            ..Labels::default()
                        },
                    },
                    to_f64(immediate.count()),
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("mean"),
                        metric_type: "anomaly_score",
                        labels: Labels {
                            immediate: Some(*immediate_interval),
                            ..Labels::default()
                        },
                    },
                    to_f64(immediate.mean()),
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("ci"),
                        metric_type: "anomaly_score",
                        labels: Labels {
                            immediate: Some(*immediate_interval),
                            ..Labels::default()
                        },
                    },
                    to_f64(immediate.confidence_interval(q)),
                );
                (
                    *immediate_interval,
                    immediate
                        .lower_bound_of_confidence_interval(q)
                        .max(from_f64(0.0)),
                )
            })
            .collect::<Vec<_>>();
        let references = self
            .reference
            .iter()
            .map(|(reference_interval, reference)| {
                metric(
                    MetricArgs {
                        metric_suffix: Some("count"),
                        metric_type: "anomaly_score",
                        labels: Labels {
                            reference: Some(*reference_interval),
                            ..Labels::default()
                        },
                    },
                    to_f64(reference.count()),
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("mean"),
                        metric_type: "anomaly_score",
                        labels: Labels {
                            reference: Some(*reference_interval),
                            ..Labels::default()
                        },
                    },
                    to_f64(reference.mean()),
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("ci"),
                        metric_type: "anomaly_score",
                        labels: Labels {
                            reference: Some(*reference_interval),
                            ..Labels::default()
                        },
                    },
                    to_f64(reference.confidence_interval(q)),
                );
                (
                    *reference_interval,
                    (reference.upper_bound_of_confidence_interval(q) + offset).value,
                )
            })
            .collect::<Vec<_>>();

        immediate
            .iter()
            .for_each(|(immediate_interval, immediate_lower_bound)| {
                references
                    .iter()
                    .for_each(|(reference_interval, reference_upper_bound)| {
                        metric(
                            MetricArgs {
                                metric_suffix: Some("score"),
                                metric_type: "anomaly_score",
                                labels: Labels {
                                    immediate: Some(*immediate_interval),
                                    reference: Some(*reference_interval),
                                    ..Labels::default()
                                },
                            },
                            to_f64((*immediate_lower_bound / *reference_upper_bound).value),
                        );
                    });
            });
    }
}

impl Default for AnomalyScoreConfig {
    fn default() -> Self {
        Self {
            reference_intervals: BTreeSet::from_iter([
                ReferenceInterval::R7d,
                ReferenceInterval::R30d,
            ]),
            immediate_intervals: BTreeSet::from_iter([
                ImmediateInterval::I5m,
                ImmediateInterval::I15m,
            ]),
            offset: NotNan::new(0.0).unwrap(),
            q: NotNan::new(0.99).unwrap(),
        }
    }
}

impl AnomalyScoreConfig {
    pub fn default_with_offset(offset: NotNan<f64>) -> Self {
        Self {
            offset,
            ..Self::default()
        }
    }
}
