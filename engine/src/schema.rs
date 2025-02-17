/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::BTreeMap;

use apistos::ApiComponent;
use prometheus_core::{LabelName, MetricName};
use prometheus_schema::{
    serial::{Histogram, Item, Metric, Module, Scalar, ScalarType, Summary},
    ItemName, ItemRef, LabelSelector, MetricSelector, ModuleName, ModuleVersion,
};
use schemars::JsonSchema;
use serde::{ser::SerializeMap, Serialize};

use crate::{
    config::Config,
    processor::{mean_stddev::MeanStddevAlgorithm, source::MetricSource},
};

pub fn get_prom_schema(config: &Config) -> Module {
    let items = std::iter::once((
        ItemName::new("root"),
        Item {
            items: config
                .trace
                .configs
                .keys()
                .map(|name| ItemRef::new(None, ItemName::new(name.to_string())))
                .collect(),
            ..Default::default()
        },
    ))
    .chain(config.trace.configs.iter().map(|(name, config)| {
        (
            ItemName::new(name.to_string()),
            Item {
                query: MetricSelector(
                    std::iter::once((
                        LabelName::new("config").unwrap(),
                        LabelSelector::Eq(name.to_string()),
                    ))
                    .chain(config.key.iter().map(|key| {
                        (
                            key.label(),
                            if key.is_required() {
                                LabelSelector::Set
                            } else {
                                LabelSelector::Opt
                            },
                        )
                    }))
                    .collect(),
                ),
                keys: std::iter::once(LabelName::new("config").unwrap())
                    .chain(config.key.iter().map(|key| key.label()))
                    .collect(),
                // items: config
                //     .metrics
                //     .keys()
                //     .map(|metric| ItemRef::new(None, ItemName::new(format!("{name}-{metric}"))))
                //     .collect(),
                metrics: {
                    let mut metrics = BTreeMap::new();
                    config.metrics.iter().for_each(|(name, config)| {
                        match &config.source {
                            MetricSource::Count { .. } | MetricSource::Rate { .. } => {
                                metrics.insert(
                                    MetricName::new(format!("trace_{name}_total")).unwrap(),
                                    Metric::Scalar(Scalar {
                                        r#type: Some(ScalarType::Counter),
                                        query: MetricSelector(
                                            std::iter::once((
                                                LabelName::new("metric_type").unwrap(),
                                                LabelSelector::Eq(String::from("source_count")),
                                            ))
                                            .collect(),
                                        ),
                                        labels: MetricSelector::new(),
                                        unit: None,
                                    }),
                                );
                            }
                            _ => {}
                        }
                        if let Some(config) = &config.stats.mean_stddev {
                            match &config.algorithm {
                                MeanStddevAlgorithm::CountSum => {
                                    metrics.insert(
                                        MetricName::new(format!("trace_{name}_count")).unwrap(),
                                        Metric::Scalar(Scalar {
                                            r#type: Some(ScalarType::Counter),
                                            query: MetricSelector(
                                                std::iter::once((
                                                    LabelName::new("metric_type").unwrap(),
                                                    LabelSelector::Eq(String::from("count_sum")),
                                                ))
                                                .collect(),
                                            ),
                                            labels: MetricSelector::new(),
                                            unit: None,
                                        }),
                                    );
                                    metrics.insert(
                                        MetricName::new(format!("trace_{name}_sum")).unwrap(),
                                        Metric::Scalar(Scalar {
                                            r#type: Some(ScalarType::Gauge),
                                            query: MetricSelector(
                                                std::iter::once((
                                                    LabelName::new("metric_type").unwrap(),
                                                    LabelSelector::Eq(String::from("count_sum")),
                                                ))
                                                .collect(),
                                            ),
                                            labels: MetricSelector::new(),
                                            unit: None,
                                        }),
                                    );
                                }
                                MeanStddevAlgorithm::Welford => {
                                    metrics.insert(
                                        MetricName::new(format!("trace_{name}_count")).unwrap(),
                                        Metric::Scalar(Scalar {
                                            r#type: Some(ScalarType::Counter),
                                            query: MetricSelector(
                                                std::iter::once((
                                                    LabelName::new("metric_type").unwrap(),
                                                    LabelSelector::Eq(String::from("welford")),
                                                ))
                                                .collect(),
                                            ),
                                            labels: MetricSelector::new(),
                                            unit: None,
                                        }),
                                    );
                                    metrics.insert(
                                        MetricName::new(format!("trace_{name}_mean")).unwrap(),
                                        Metric::Scalar(Scalar {
                                            r#type: Some(ScalarType::Gauge),
                                            query: MetricSelector(
                                                std::iter::once((
                                                    LabelName::new("metric_type").unwrap(),
                                                    LabelSelector::Eq(String::from("welford")),
                                                ))
                                                .collect(),
                                            ),
                                            labels: MetricSelector::new(),
                                            unit: None,
                                        }),
                                    );
                                    metrics.insert(
                                        MetricName::new(format!("trace_{name}_m2")).unwrap(),
                                        Metric::Scalar(Scalar {
                                            r#type: Some(ScalarType::Gauge),
                                            query: MetricSelector(
                                                std::iter::once((
                                                    LabelName::new("metric_type").unwrap(),
                                                    LabelSelector::Eq(String::from("welford")),
                                                ))
                                                .collect(),
                                            ),
                                            labels: MetricSelector::new(),
                                            unit: None,
                                        }),
                                    );
                                }
                            }
                        }
                        if config.stats.summary.is_some() {
                            metrics.insert(
                                MetricName::new(format!("trace_{name}")).unwrap(),
                                Metric::Summary(Summary {
                                    query: MetricSelector(
                                        std::iter::once((
                                            LabelName::new("metric_type").unwrap(),
                                            LabelSelector::Eq(String::from("summary")),
                                        ))
                                        .collect(),
                                    ),
                                    labels: MetricSelector::new(),
                                    unit: None,
                                }),
                            );
                        }
                        if config.stats.histogram.is_some() {
                            metrics.insert(
                                MetricName::new(format!("trace_{name}")).unwrap(),
                                Metric::Histogram(Histogram {
                                    query: MetricSelector(
                                        std::iter::once((
                                            LabelName::new("metric_type").unwrap(),
                                            LabelSelector::Eq(String::from("histogram")),
                                        ))
                                        .collect(),
                                    ),
                                    labels: MetricSelector::new(),
                                    unit: None,
                                }),
                            );
                        }
                    });
                    metrics
                },
                ..Default::default()
            },
        )
        //     .chain(config.metrics.iter().map(move |(metric, config)| {
        //         (
        //             ItemName::new(format!("{name}-{metric}")),
        //             Item {
        //                 ..Default::default()
        //             },
        //         )
        //     }))
    }))
    .collect();

    Module {
        version: ModuleVersion::new("0.1.0".parse().unwrap()),
        requires: BTreeMap::new(),
        items,
    }

    //PromSchema(Singleton(ModuleName::new("jaeger-stats"), schema))
}

#[derive(Serialize, JsonSchema, ApiComponent)]
pub struct PromSchema(Singleton<ModuleName, Module>);

struct Singleton<K, V>(K, V);

impl<K: Serialize, V: Serialize> Serialize for Singleton<K, V> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&self.0, &self.1)?;
        map.end()
    }
}

// Adapted from auto-derived version.
impl<K, V> schemars::JsonSchema for Singleton<K, V>
where
    K: JsonSchema,
    V: JsonSchema,
{
    fn schema_name() -> std::string::String {
        format!(
            "Singleton_for_{}_and_{}",
            K::schema_name(),
            V::schema_name()
        )
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Owned(format!(
            std::concat!(std::module_path!(), "::", "Singleton_for_{}_and_{}"),
            K::schema_id(),
            V::schema_id()
        ))
    }
    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::Schema::Object(schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::Object.into()),
            object: Some(Box::new(schemars::schema::ObjectValidation {
                pattern_properties: BTreeMap::from_iter([(
                    String::from(".*"),
                    V::json_schema(gen),
                )]),
                max_properties: Some(1u32),
                min_properties: Some(1u32),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}
