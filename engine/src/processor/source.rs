/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use chrono::{DateTime, Utc};
use jaeger_anomaly_detection::WindowConfig;
use serde::{Deserialize, Serialize};

use crate::{
    accum::{Accum, Count, MergeAcc},
    config::SpanSelector,
    jaeger::Span,
    metrics::Labels,
    window::Window,
};

use super::metric::MetricArgs;

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    Tag(String),
    Duration,
    SelfDuration,
    TagExcept { tag: String, key: String },
    Rate { select: SpanSelector },
    Count { window: WindowConfig },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SourceState {
    Count { window: Window<Count>, count: u64 },
}

pub enum SourceProcessor {
    /* Numeric sources.  */
    SelfDuration,
    Duration,
    Tag(String),
    TagExcept(String, String),
    Rate(SpanSelector),

    /* Windowed sources. */
    Count { window: Window<Count>, count: u64 },
}

impl SourceProcessor {
    pub fn new(t: DateTime<Utc>, config: &MetricSource) -> Self {
        match config {
            MetricSource::Tag(name) => SourceProcessor::Tag(name.clone()),
            MetricSource::Duration => SourceProcessor::Duration,
            MetricSource::SelfDuration => SourceProcessor::SelfDuration,
            MetricSource::TagExcept { tag, key } => {
                SourceProcessor::TagExcept(tag.clone(), key.clone())
            }
            MetricSource::Rate { select } => SourceProcessor::Rate(select.clone()),
            MetricSource::Count { window } => SourceProcessor::Count {
                window: Window::new(t, window),
                count: 0,
            },
        }
    }

    pub fn update(self, _t: DateTime<Utc>, config: &MetricSource) -> Option<SourceProcessor> {
        match (self, config) {
            (SourceProcessor::Tag(prev), MetricSource::Tag(name)) if name == &prev => {
                Some(SourceProcessor::Tag(prev))
            }
            (
                SourceProcessor::TagExcept(prev_tag, prev_key),
                MetricSource::TagExcept { tag, key },
            ) if tag == &prev_tag && key == &prev_key => {
                Some(SourceProcessor::TagExcept(prev_tag, prev_key))
            }
            (SourceProcessor::SelfDuration, MetricSource::SelfDuration) => {
                Some(SourceProcessor::SelfDuration)
            }
            (SourceProcessor::Rate(prev_select), MetricSource::Rate { select })
                if select == &prev_select =>
            {
                Some(SourceProcessor::Rate(prev_select))
            }
            (
                SourceProcessor::Count { window, count },
                MetricSource::Count {
                    window: window_config,
                },
            ) if window.compatible_with(window_config) => Some(SourceProcessor::Count {
                window: window.clone(),
                count,
            }),
            _ => None,
        }
    }

    pub fn load(t: DateTime<Utc>, state: Option<SourceState>, config: &MetricSource) -> Self {
        match (config, state) {
            (
                MetricSource::Count {
                    window: window_config,
                },
                Some(SourceState::Count { window, count }),
            ) if window_config.bin_width.to_time_delta() == window.bin_width()
                && window_config.num_bins == window.num_bins() =>
            {
                Self::Count { window, count }
            }
            _ => Self::new(t, config),
        }
    }

    pub fn save(&self) -> Option<SourceState> {
        match self {
            SourceProcessor::SelfDuration
            | SourceProcessor::Duration
            | SourceProcessor::Tag(_)
            | SourceProcessor::TagExcept(_, _)
            | SourceProcessor::Rate(_) => None,
            SourceProcessor::Count { window, count } => Some(SourceState::Count {
                window: window.clone(),
                count: *count,
            }),
        }
    }

    pub fn insert<F: FnMut(f64)>(
        &mut self,
        t: DateTime<Utc>,
        span: &Span,
        parent: Option<&Span>,
        children: &[&Span],
        mut f: F,
    ) {
        match self {
            Self::Duration => f(span.duration as f64),
            Self::SelfDuration => {
                // Calculate time not spent in any child spans. The list
                // of child spans is ordered by start_time.
                let span_end_time = span.start_time + span.duration;
                let self_duration = children
                    .iter()
                    .fold(
                        (span.duration, span.start_time),
                        |(sum, max_end_time), child| {
                            let child_end_time = child.start_time + child.duration;
                            (
                                sum - child.duration
                                    + (max_end_time - child.start_time).max(0).min(child.duration)
                                    + (child_end_time - span_end_time).max(0).min(child.duration)
                                    - (max_end_time - span_end_time).max(0).min(child.duration),
                                max_end_time.max(child_end_time),
                            )
                        },
                    )
                    .0;
                f(self_duration as f64)
            }
            Self::Tag(name) => {
                if let Some(n) = span
                    .tags
                    .iter()
                    .find(|tag| &tag.key == name)
                    .and_then(|tag| tag.value.as_int())
                {
                    f(n as f64)
                }
            }
            Self::TagExcept(name, key) => {
                if let Some(n) = span
                    .tags
                    .iter()
                    .find(|tag| &tag.key == name)
                    .and_then(|tag| tag.value.as_int())
                {
                    let id = span
                        .tags
                        .iter()
                        .find(|tag| &tag.key == key)
                        .map(|tag| &tag.value);

                    let cn = children
                        .iter()
                        .filter(|span| {
                            id.map_or(true, |id| {
                                span.tags
                                    .iter()
                                    .find(|tag| &tag.key == key)
                                    .map_or(true, |tag| &tag.value == id)
                            })
                        })
                        .filter_map(|span| {
                            span.tags
                                .iter()
                                .find(|tag| &tag.key == name)
                                .and_then(|tag| tag.value.as_int())
                        })
                        .sum::<i64>();
                    f((n - cn) as f64)
                }
            }
            Self::Rate(select) => f(if select.matches(span, parent) {
                1.0
            } else {
                0.0
            }),

            Self::Count { window, count } => {
                window
                    .advance_with(t, |window| {
                        window.bins().merge().extract() as f64 / window.minutes()
                    })
                    .for_each(f);
                *count += 1;
                window.current_mut().insert(());
            }
        }
    }

    pub fn sample<F: for<'b> FnMut(MetricArgs, f64)>(&self, _t: DateTime<Utc>, mut metric: F) {
        match self {
            Self::Count { count, .. } => {
                metric(
                    MetricArgs {
                        metric_suffix: Some("total"),
                        metric_type: "source_count",
                        labels: Labels::default(),
                    },
                    *count as f64,
                );
            }
            Self::SelfDuration
            | Self::Duration
            | Self::Tag(_)
            | Self::TagExcept(_, _)
            | Self::Rate(_) => {}
        }
    }
}
