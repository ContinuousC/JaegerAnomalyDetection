/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use rustc_apfloat::ieee::Quad;
use serde::{Deserialize, Serialize};

use crate::{accum::Accum, metrics::Labels, welford::Welford};

use super::metric::MetricArgs;

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct MeanStddevConfig {
    pub algorithm: MeanStddevAlgorithm,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MeanStddevAlgorithm {
    CountSum,
    Welford,
}

pub type MeanStddevState = MeanStddevProcessor;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MeanStddevProcessor {
    CountSum(u64, f64),
    Welford(Welford<Quad>),
}

impl MeanStddevProcessor {
    pub fn new(config: &MeanStddevConfig) -> Self {
        match &config.algorithm {
            MeanStddevAlgorithm::CountSum => Self::CountSum(0, 0.0),
            MeanStddevAlgorithm::Welford => Self::Welford(Welford::default()),
        }
    }

    pub fn update(&self, config: &MeanStddevConfig) -> MeanStddevProcessor {
        match (self, &config.algorithm) {
            (Self::CountSum(count, sum), MeanStddevAlgorithm::CountSum) => {
                Self::CountSum(*count, *sum)
            }
            (Self::Welford(acc), MeanStddevAlgorithm::Welford) => Self::Welford(acc.clone()),
            _ => Self::new(config),
        }
    }

    pub fn load(state: Self, config: &MeanStddevConfig) -> Self {
        match (config.algorithm, state) {
            (MeanStddevAlgorithm::CountSum, state @ Self::CountSum(_, _)) => state,
            (MeanStddevAlgorithm::Welford, state @ Self::Welford(_)) => state,
            _ => Self::new(config),
        }
    }

    pub fn save(&self) -> Self {
        self.clone()
    }

    pub fn insert(&mut self, value: f64) {
        match self {
            MeanStddevProcessor::CountSum(count, sum) => {
                *count += 1;
                *sum += value;
            }
            MeanStddevProcessor::Welford(acc) => acc.insert(value),
        }
    }

    pub fn sample<F: FnMut(MetricArgs, f64)>(&self, mut metric: F) {
        match self {
            MeanStddevProcessor::CountSum(count, sum) => {
                metric(
                    MetricArgs {
                        metric_suffix: Some("count"),
                        metric_type: "count_sum",
                        labels: Labels::default(),
                    },
                    *count as f64,
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("sum"),
                        metric_type: "count_sum",
                        labels: Labels::default(),
                    },
                    *sum,
                );
            }
            MeanStddevProcessor::Welford(welford) => {
                let welford = welford.extract();
                metric(
                    MetricArgs {
                        metric_suffix: Some("count"),
                        metric_type: "welford",
                        labels: Labels::default(),
                    },
                    welford.count,
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("mean"),
                        metric_type: "welford",
                        labels: Labels::default(),
                    },
                    welford.mean,
                );
                metric(
                    MetricArgs {
                        metric_suffix: Some("m2"),
                        metric_type: "welford",
                        labels: Labels::default(),
                    },
                    welford.m2,
                );
            }
        }
    }
}

impl Default for MeanStddevConfig {
    fn default() -> Self {
        Self {
            algorithm: MeanStddevAlgorithm::Welford,
        }
    }
}
