/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use serde::{Deserialize, Serialize};

use crate::metrics::Labels;

use super::metric::MetricArgs;

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
pub struct HistogramConfig {
    pub bounds: Vec<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistogramState {
    bins: Vec<f64>,
    count: u64,
    sum: f64,
}

pub struct HistogramProcessor {
    bounds: Vec<f64>,
    bins: Vec<f64>,
    count: u64,
    sum: f64,
}

impl HistogramProcessor {
    pub fn new(config: &HistogramConfig) -> Self {
        Self {
            bounds: config.bounds.clone(),
            bins: std::iter::repeat(0.0).take(config.bounds.len()).collect(),
            count: 0,
            sum: 0.0,
        }
    }

    pub fn load(state: HistogramState, config: &HistogramConfig) -> Self {
        Self {
            bounds: config.bounds.clone(),
            bins: state.bins,
            count: state.count,
            sum: state.sum,
        }
    }

    pub fn save(&self) -> HistogramState {
        HistogramState {
            bins: self.bins.clone(),
            count: self.count,
            sum: self.sum,
        }
    }

    pub fn update(&self, config: &HistogramConfig) -> HistogramProcessor {
        if self.bounds == config.bounds {
            HistogramProcessor {
                bounds: config.bounds.clone(),
                bins: self.bins.clone(),
                count: self.count,
                sum: self.sum,
            }
        } else {
            HistogramProcessor::new(config)
        }
    }

    pub fn insert(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.bounds
            .iter()
            .copied()
            .zip(&mut self.bins)
            .take_while(|(bound, _)| value <= *bound)
            .for_each(|(_, count)| *count += 1.0);
    }

    pub fn sample<F: FnMut(MetricArgs, f64)>(&self, mut metric: F) {
        metric(
            MetricArgs {
                metric_suffix: Some("count"),
                metric_type: "histogram",
                labels: Labels::default(),
            },
            self.count as f64,
        );
        metric(
            MetricArgs {
                metric_suffix: Some("sum"),
                metric_type: "histogram",
                labels: Labels::default(),
            },
            self.sum,
        );
        self.bounds.iter().zip(&self.bins).for_each(|(bound, n)| {
            metric(
                MetricArgs {
                    metric_suffix: Some("buckets"),
                    metric_type: "histogram",
                    labels: Labels {
                        le: Some(format!("{bound:.0}")),
                        ..Labels::default()
                    },
                },
                *n,
            );
        });
    }
}
