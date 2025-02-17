/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use chrono::{DateTime, Utc};
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

use super::{
    anomaly_score::{AnomalyScoreConfig, AnomalyScoreProcessor, AnomalyScoreState},
    histogram::{HistogramConfig, HistogramProcessor, HistogramState},
    mean_stddev::{MeanStddevConfig, MeanStddevProcessor, MeanStddevState},
    metric::MetricArgs,
    summary::{SummaryConfig, SummaryProcessor, SummaryState},
};

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
pub struct StatsConfig {
    pub anomaly_score: Option<AnomalyScoreConfig>,
    pub mean_stddev: Option<MeanStddevConfig>,
    pub summary: Option<SummaryConfig>,
    pub histogram: Option<HistogramConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StatsState {
    anomaly_score: Option<AnomalyScoreState>,
    mean_stddev: Option<MeanStddevState>,
    summary: Option<SummaryState>,
    histogram: Option<HistogramState>,
}

pub struct StatsProcessor {
    anomaly_score: Option<AnomalyScoreProcessor>,
    mean_stddev: Option<MeanStddevProcessor>,
    summary: Option<SummaryProcessor>,
    histogram: Option<HistogramProcessor>,
}

impl StatsProcessor {
    pub fn new(t: DateTime<Utc>, config: &StatsConfig) -> Self {
        Self {
            anomaly_score: config
                .anomaly_score
                .as_ref()
                .map(|config| AnomalyScoreProcessor::new(t, config)),
            mean_stddev: config.mean_stddev.as_ref().map(MeanStddevProcessor::new),
            histogram: config.histogram.as_ref().map(HistogramProcessor::new),
            summary: config
                .summary
                .as_ref()
                .map(|config| SummaryProcessor::new(t, config)),
        }
    }

    pub fn update(self, t: DateTime<Utc>, config: &StatsConfig) -> StatsProcessor {
        StatsProcessor {
            anomaly_score: config.anomaly_score.as_ref().map(|config| {
                self.anomaly_score.map_or_else(
                    || AnomalyScoreProcessor::new(t, config),
                    |proc| proc.update(t, config),
                )
            }),
            mean_stddev: config.mean_stddev.as_ref().map(|config| {
                self.mean_stddev.map_or_else(
                    || MeanStddevProcessor::new(config),
                    |proc| proc.update(config),
                )
            }),
            histogram: config.histogram.as_ref().map(|config| {
                self.histogram.map_or_else(
                    || HistogramProcessor::new(config),
                    |proc| proc.update(config),
                )
            }),
            summary: config.summary.as_ref().map(|config| {
                self.summary.map_or_else(
                    || SummaryProcessor::new(t, config),
                    |proc| proc.update(t, config),
                )
            }),
        }
    }

    pub fn load(t: DateTime<Utc>, state: StatsState, config: &StatsConfig) -> Self {
        Self {
            anomaly_score: config.anomaly_score.as_ref().map(|config| {
                state.anomaly_score.map_or_else(
                    || AnomalyScoreProcessor::new(t, config),
                    |state| AnomalyScoreProcessor::load(t, state, config),
                )
            }),
            mean_stddev: config.mean_stddev.as_ref().map(|config| {
                state.mean_stddev.map_or_else(
                    || MeanStddevProcessor::new(config),
                    |state| MeanStddevProcessor::load(state, config),
                )
            }),
            summary: config.summary.as_ref().map(|config| {
                state.summary.map_or_else(
                    || SummaryProcessor::new(t, config),
                    |state| SummaryProcessor::load(t, state, config),
                )
            }),
            histogram: config.histogram.as_ref().map(|config| {
                state.histogram.map_or_else(
                    || HistogramProcessor::new(config),
                    |state| HistogramProcessor::load(state, config),
                )
            }),
        }
    }

    pub fn save(&self) -> StatsState {
        StatsState {
            anomaly_score: self.anomaly_score.as_ref().map(|proc| proc.save()),
            mean_stddev: self.mean_stddev.as_ref().map(|proc| proc.save()),
            summary: self.summary.as_ref().map(|proc| proc.save()),
            histogram: self.histogram.as_ref().map(|proc| proc.save()),
        }
    }

    pub fn insert(&mut self, t: DateTime<Utc>, value: f64) {
        if let Some(acc) = &mut self.anomaly_score {
            acc.insert(t, value);
        }
        if let Some(acc) = &mut self.mean_stddev {
            acc.insert(value);
        }
        if let Some(acc) = &mut self.summary {
            acc.insert(value);
        }
        if let Some(acc) = &mut self.histogram {
            acc.insert(value);
        }
    }

    pub fn sample<F: FnMut(MetricArgs, f64)>(&self, _t: DateTime<Utc>, mut metric: F) {
        if let Some(proc) = self.anomaly_score.as_ref() {
            proc.sample(&mut metric)
        }
        if let Some(proc) = self.mean_stddev.as_ref() {
            proc.sample(&mut metric)
        }
        if let Some(proc) = self.summary.as_ref() {
            proc.sample(&mut metric)
        }
        if let Some(proc) = self.histogram.as_ref() {
            proc.sample(&mut metric)
        }
    }
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            anomaly_score: Some(AnomalyScoreConfig::default()),
            mean_stddev: Some(MeanStddevConfig::default()),
            summary: Some(SummaryConfig::default()),
            histogram: None,
        }
    }
}

impl StatsConfig {
    pub fn default_with_offset(offset: NotNan<f64>) -> Self {
        Self {
            anomaly_score: Some(AnomalyScoreConfig::default_with_offset(offset)),
            mean_stddev: Some(MeanStddevConfig::default()),
            summary: Some(SummaryConfig::default()),
            histogram: None,
        }
    }
}
