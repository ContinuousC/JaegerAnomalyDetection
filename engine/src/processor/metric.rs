/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{jaeger::Span, metrics::Labels};

use super::{
    source::{MetricSource, SourceProcessor, SourceState},
    stats::{StatsConfig, StatsProcessor, StatsState},
};

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
pub struct MetricConfig {
    pub source: MetricSource,
    pub stats: StatsConfig,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricState {
    source: Option<SourceState>,
    stats: StatsState,
}

pub struct MetricProcessor {
    source: SourceProcessor,
    stats: StatsProcessor,
}

impl MetricProcessor {
    pub fn new(t: DateTime<Utc>, config: &MetricConfig) -> Self {
        Self {
            source: SourceProcessor::new(t, &config.source),
            stats: StatsProcessor::new(t, &config.stats),
        }
    }

    pub fn update(self, t: DateTime<Utc>, config: &MetricConfig) -> Self {
        if let Some(source) = self.source.update(t, &config.source) {
            MetricProcessor {
                source,
                stats: self.stats.update(t, &config.stats),
            }
        } else {
            MetricProcessor::new(t, config)
        }
    }

    pub fn load(t: DateTime<Utc>, state: MetricState, config: &MetricConfig) -> Self {
        Self {
            source: SourceProcessor::load(t, state.source, &config.source),
            stats: StatsProcessor::load(t, state.stats, &config.stats),
        }
    }

    pub fn save(&self) -> MetricState {
        MetricState {
            source: self.source.save(),
            stats: self.stats.save(),
        }
    }

    pub fn insert(
        &mut self,
        t: DateTime<Utc>,
        span: &Span,
        parent: Option<&Span>,
        children: &[&Span],
    ) {
        self.source
            .insert(t, span, parent, children, |v| self.stats.insert(t, v))
    }

    pub fn sample<F: FnMut(MetricArgs, f64)>(&self, t: DateTime<Utc>, mut metric: F) {
        self.source.sample(t, &mut metric);
        self.stats.sample(t, &mut metric);
    }
}

pub(crate) struct MetricArgs {
    pub(crate) metric_suffix: Option<&'static str>,
    pub(crate) metric_type: &'static str,
    pub(crate) labels: Labels,
}
