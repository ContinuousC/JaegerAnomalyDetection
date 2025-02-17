/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use chrono::{DateTime, Utc};
use jaeger_anomaly_detection::WindowConfig;
use serde::{Deserialize, Serialize};
use tdigest::TDigest;

use crate::{accum::MergeAcc, metrics::Labels, window::Window};

use super::metric::MetricArgs;

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
pub struct SummaryConfig {
    pub window: WindowConfig,
    pub percentiles: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SummaryState {
    window: Window<TDigest>,
    count: u64,
    sum: f64,
}

pub struct SummaryProcessor {
    percentiles: Vec<f64>,
    window: Window<TDigest>,
    count: u64,
    sum: f64,
}

impl SummaryProcessor {
    pub fn new(t: DateTime<Utc>, config: &SummaryConfig) -> Self {
        Self {
            percentiles: config.percentiles.clone(),
            window: Window::new(t, &config.window),
            count: 0,
            sum: 0.0,
        }
    }

    pub fn update(&self, t: DateTime<Utc>, config: &SummaryConfig) -> SummaryProcessor {
        if self.window.compatible_with(&config.window) {
            SummaryProcessor {
                percentiles: config.percentiles.clone(),
                window: self.window.clone(),
                count: self.count,
                sum: self.sum,
            }
        } else {
            SummaryProcessor::new(t, config)
        }
    }

    pub fn load(t: DateTime<Utc>, state: SummaryState, config: &SummaryConfig) -> Self {
        if state.window.compatible_with(&config.window) {
            Self {
                percentiles: config.percentiles.clone(),
                window: state.window,
                count: state.count,
                sum: state.sum,
            }
        } else {
            Self::new(t, config)
        }
    }

    pub fn save(&self) -> SummaryState {
        SummaryState {
            window: self.window.clone(),
            count: self.count,
            sum: self.sum,
        }
    }

    pub fn insert(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        let tdigest = self.window.current_mut();
        *tdigest = tdigest.merge_sorted(vec![value]);
    }

    pub fn sample<F: FnMut(MetricArgs, f64)>(&self, mut metric: F) {
        metric(
            MetricArgs {
                metric_suffix: Some("count"),
                metric_type: "summary",
                labels: Labels::default(),
            },
            self.count as f64,
        );
        metric(
            MetricArgs {
                metric_suffix: Some("sum"),
                metric_type: "summary",
                labels: Labels::default(),
            },
            self.sum,
        );
        let tdigest = self.window.bins().merge();
        for q in &self.percentiles {
            metric(
                MetricArgs {
                    metric_suffix: None,
                    metric_type: "summary",
                    labels: Labels {
                        q: Some(format!("{q:.2}")),
                        ..Labels::default()
                    },
                },
                tdigest.estimate_quantile(*q),
            );
        }
    }
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            percentiles: vec![0.5, 0.95, 0.99],
        }
    }
}
