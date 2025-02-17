/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use jaeger_anomaly_detection::WindowConfig;
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

use crate::{
    config::{
        ConfigName, KeyName, LowerBound, MetricName, Range, Regex, SpanKey, SpanSelector,
        UpperBound,
    },
    jaeger::{RefType, Span, TagValue},
    metrics::Labels,
};

use super::{
    metric::MetricConfig,
    source::MetricSource,
    span::{SpanConfig, SpanProcessor, SpanState},
    stats::StatsConfig,
};

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
#[serde(default)]
pub struct TraceConfig {
    pub rules: Vec<Vec<Rule>>,
    pub configs: BTreeMap<ConfigName, SpanConfig>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Clone, Debug)]
pub struct Rule {
    pub select: SpanSelector,
    pub config: ConfigName,
}

pub(crate) struct MetricArgs<'a> {
    pub(crate) metric_name: String,
    pub(crate) metric_type: &'static str,
    pub(crate) labels: Labels,
    pub(crate) key: &'a BTreeMap<SpanKey, TagValue>,
}

impl Default for TraceConfig {
    fn default() -> Self {
        TraceConfig {
            rules: Vec::from([
                Vec::from([Rule {
                    select: SpanSelector::All(Vec::new()),
                    config: ConfigName::new("default"),
                }]),
                Vec::from([Rule {
                    select: SpanSelector::Has(SpanKey::Parent(KeyName::Duration)),
                    config: ConfigName::new("operation-relations"),
                }]),
                Vec::from([Rule {
                    select: SpanSelector::All(Vec::from_iter([
                        SpanSelector::Has(SpanKey::Parent(KeyName::Duration)),
                        SpanSelector::Any(Vec::from_iter([
                            SpanSelector::KeyNe(
                                SpanKey::Current(KeyName::ServiceName),
                                SpanKey::Parent(KeyName::ServiceName),
                            ),
                            SpanSelector::KeyNe(
                                SpanKey::Current(KeyName::ProcessTag(String::from(
                                    "service.namespace",
                                ))),
                                SpanKey::Parent(KeyName::ProcessTag(String::from(
                                    "service.namespace",
                                ))),
                            ),
                            SpanSelector::KeyNe(
                                SpanKey::Current(KeyName::ProcessTag(String::from(
                                    "service.instance.id",
                                ))),
                                SpanKey::Parent(KeyName::ProcessTag(String::from(
                                    "service.instance.id",
                                ))),
                            ),
                        ])),
                    ])),
                    config: ConfigName::new("service-relations"),
                }]),
            ]),
            configs: BTreeMap::from_iter([
                (
                    ConfigName::new("default"),
                    SpanConfig {
                        key: BTreeSet::from_iter([
                            SpanKey::Current(KeyName::ServiceName),
                            SpanKey::Current(KeyName::OperationName),
                            SpanKey::Current(KeyName::ProcessTag(String::from(
                                "service.namespace",
                            ))),
                            SpanKey::Current(KeyName::ProcessTag(String::from(
                                "service.instance.id",
                            ))),
                        ]),
                        metrics: BTreeMap::from_iter([
                            (
                                MetricName::new("duration"),
                                MetricConfig {
                                    source: MetricSource::SelfDuration,
                                    stats: StatsConfig::default_with_offset(
                                        NotNan::new(1000.0).unwrap(),
                                    ),
                                },
                            ),
                            (
                                MetricName::new("busy"),
                                MetricConfig {
                                    source: MetricSource::TagExcept {
                                        tag: String::from("busy_ns"),
                                        key: String::from("thread.id"),
                                    },
                                    stats: StatsConfig::default_with_offset(
                                        NotNan::new(1_000_000.0).unwrap(),
                                    ),
                                },
                            ),
                            (
                                MetricName::new("call_rate"),
                                MetricConfig {
                                    source: MetricSource::Count {
                                        window: WindowConfig::default(),
                                    },
                                    stats: StatsConfig::default_with_offset(
                                        NotNan::new(1.0).unwrap(),
                                    ),
                                },
                            ),
                            (
                                MetricName::new("error_rate"),
                                MetricConfig {
                                    source: MetricSource::Rate {
                                        select: SpanSelector::Any(vec![
                                            SpanSelector::IsTrue(SpanKey::Current(
                                                KeyName::SpanTag(String::from("error")),
                                            )),
                                            SpanSelector::Has(SpanKey::Current(KeyName::SpanTag(
                                                String::from("exception.message"),
                                            ))),
                                            SpanSelector::Outside(
                                                SpanKey::Current(KeyName::SpanTag(String::from(
                                                    "http.status_code",
                                                ))),
                                                Range {
                                                    lower: Some(LowerBound::Ge(200)),
                                                    upper: Some(UpperBound::Le(299)),
                                                },
                                            ),
                                            SpanSelector::NoMatch(
                                                SpanKey::Current(KeyName::SpanTag(String::from(
                                                    "http.status_code",
                                                ))),
                                                Regex::new("^2..$").unwrap(),
                                            ),
                                        ]),
                                    },
                                    stats: StatsConfig::default_with_offset(
                                        NotNan::new(0.01).unwrap(),
                                    ),
                                },
                            ),
                        ]),
                    },
                ),
                (
                    ConfigName::new("operation-relations"),
                    SpanConfig {
                        key: BTreeSet::from_iter([
                            SpanKey::Current(KeyName::ServiceName),
                            SpanKey::Current(KeyName::OperationName),
                            SpanKey::Current(KeyName::ProcessTag(String::from(
                                "service.namespace",
                            ))),
                            SpanKey::Current(KeyName::ProcessTag(String::from(
                                "service.instance.id",
                            ))),
                            SpanKey::Parent(KeyName::ServiceName),
                            SpanKey::Parent(KeyName::OperationName),
                            SpanKey::Parent(KeyName::ProcessTag(String::from("service.namespace"))),
                            SpanKey::Parent(KeyName::ProcessTag(String::from(
                                "service.instance.id",
                            ))),
                        ]),
                        metrics: BTreeMap::from_iter([(
                            MetricName::new("duration"),
                            MetricConfig {
                                source: MetricSource::Duration,
                                stats: StatsConfig::default_with_offset(
                                    NotNan::new(1000.0).unwrap(),
                                ),
                            },
                        )]),
                    },
                ),
                (
                    ConfigName::new("service-relations"),
                    SpanConfig {
                        key: BTreeSet::from_iter([
                            SpanKey::Current(KeyName::ServiceName),
                            SpanKey::Current(KeyName::ProcessTag(String::from(
                                "service.namespace",
                            ))),
                            SpanKey::Current(KeyName::ProcessTag(String::from(
                                "service.instance.id",
                            ))),
                            SpanKey::Parent(KeyName::ServiceName),
                            SpanKey::Parent(KeyName::ProcessTag(String::from("service.namespace"))),
                            SpanKey::Parent(KeyName::ProcessTag(String::from(
                                "service.instance.id",
                            ))),
                        ]),
                        metrics: BTreeMap::from_iter([(
                            MetricName::new("duration"),
                            MetricConfig {
                                source: MetricSource::Duration,
                                stats: StatsConfig::default_with_offset(
                                    NotNan::new(1000.0).unwrap(),
                                ),
                            },
                        )]),
                    },
                ),
            ]),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TraceState {
    groups: BTreeMap<ConfigName, SpanState>,
}

pub struct TraceProcessor {
    rules: Vec<Vec<Rule>>,
    groups: BTreeMap<ConfigName, SpanProcessor>,
}

impl TraceProcessor {
    pub fn new(config: &TraceConfig) -> Self {
        Self {
            rules: config.rules.clone(),
            groups: config
                .configs
                .iter()
                .map(|(name, config)| (name.clone(), SpanProcessor::new(config)))
                .collect(),
        }
    }

    pub fn update(mut self, t: DateTime<Utc>, config: &TraceConfig) -> TraceProcessor {
        TraceProcessor {
            rules: config.rules.clone(),
            groups: config
                .configs
                .iter()
                .map(|(name, config)| {
                    if let Some(proc) = self.groups.remove(name) {
                        (name.clone(), proc.update(t, config))
                    } else {
                        (name.clone(), SpanProcessor::new(config))
                    }
                })
                .collect(),
        }
    }

    pub fn load(t: DateTime<Utc>, mut state: TraceState, config: &TraceConfig) -> Self {
        Self {
            rules: config.rules.clone(),
            groups: config
                .configs
                .iter()
                .map(|(name, config)| {
                    (
                        name.clone(),
                        if let Some(state) = state.groups.remove(name) {
                            SpanProcessor::load(t, state, config)
                        } else {
                            SpanProcessor::new(config)
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn save(&self) -> TraceState {
        TraceState {
            groups: self
                .groups
                .iter()
                .map(|(name, proc)| ((*name).clone(), proc.save()))
                .collect(),
        }
    }

    pub fn insert(&mut self, t: DateTime<Utc>, trace: &[Span]) {
        let spans = trace
            .iter()
            .map(|span| (&span.span_id, span))
            .collect::<BTreeMap<_, _>>();
        let parents = trace
            .iter()
            .filter_map(|span| {
                let parent = &span
                    .references
                    .iter()
                    .find(|r| r.ref_type == RefType::ChildOf)?
                    .span_id;
                Some((&span.span_id, *spans.get(parent)?))
            })
            .collect::<BTreeMap<_, _>>();
        let children = trace
            .iter()
            .filter_map(|span| {
                let parent = &span
                    .references
                    .iter()
                    .find(|r| r.ref_type == RefType::ChildOf)?
                    .span_id;
                Some((parent, span))
            })
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut map, (parent, span)| {
                map.entry(parent).or_default().push(span);
                map
            });
        trace.iter().for_each(|span| {
            for rule in self.rules.iter().filter_map(|rules| {
                rules.iter().find(|rule| {
                    rule.select
                        .matches(span, parents.get(&span.span_id).copied())
                })
            }) {
                let parent = parents.get(&span.span_id).copied();
                let children: &[&Span] = children.get(&span.span_id).map_or(&[], |cs| cs);
                if let Some(proc) = self.groups.get_mut(&rule.config) {
                    proc.insert(t, span, parent, children);
                }
            }
        })
    }

    pub fn sample<F: FnMut(MetricArgs<'_>, &ConfigName, f64)>(
        &mut self,
        t: DateTime<Utc>,
        mut metric: F,
    ) {
        self.groups.iter_mut().for_each(|(config_name, proc)| {
            proc.sample(t, |metric_args, value| {
                metric(metric_args, config_name, value);
            });
        })
    }

    pub fn cleanup(&mut self, t: DateTime<Utc>) {
        self.groups.values_mut().for_each(|proc| proc.cleanup(t));
    }
}
