/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use const_format::formatcp;
use ordered_float::NotNan;
use prometheus_core::{LabelName, MetricName};
use prometheus_expr::{Expr, LabelSelector, MetricSelector, PromSelect, SelectItem};
use serde::{Deserialize, Serialize};
use serde_with::{with_prefix, DeserializeFromStr, SerializeDisplay};
use unit::{FracPrefix, TimeUnit, Unit, NEUTRAL_UNIT};

use crate::{anomaly_score::Interval, ImmediateInterval, ReferenceInterval};

#[derive(Serialize, Deserialize, Debug)]
pub struct TraceExpr {
    metric: TraceMetric,
    aggr: TraceAggr,
}

#[derive(SerializeDisplay, DeserializeFromStr, PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(from_wasm_abi, into_wasm_abi))]
#[cfg_attr(
    any(feature = "schemars", feature = "tsify"),
    serde(rename_all = "snake_case")
)]
pub enum TraceMetric {
    Duration,
    Busy,
    CallRate,
    ErrorRate,
}

impl TraceMetric {
    pub const fn metric(&self) -> MetricName {
        match self {
            TraceMetric::Duration => MetricName::new_static("duration"),
            TraceMetric::Busy => MetricName::new_static("busy"),
            TraceMetric::CallRate => MetricName::new_static("call_rate"),
            TraceMetric::ErrorRate => MetricName::new_static("error_rate"),
        }
    }

    pub const fn unit(&self) -> Unit {
        match self {
            TraceMetric::Duration => Unit::Time(TimeUnit::Second(FracPrefix::Micro)),
            TraceMetric::Busy => Unit::Time(TimeUnit::Second(FracPrefix::Nano)),
            TraceMetric::CallRate => Unit::Frequency(unit::FrequencyUnit::PerTime(
                TimeUnit::Second(FracPrefix::Unit),
            )),
            TraceMetric::ErrorRate => NEUTRAL_UNIT,
        }
    }
}

impl Display for TraceMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceMetric::Duration => write!(f, "duration"),
            TraceMetric::Busy => write!(f, "busy"),
            TraceMetric::CallRate => write!(f, "call_rate"),
            TraceMetric::ErrorRate => write!(f, "error_rate"),
        }
    }
}

impl FromStr for TraceMetric {
    type Err = TraceMetricParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "duration" => Ok(Self::Duration),
            "busy" => Ok(Self::Busy),
            "call_rate" => Ok(Self::CallRate),
            "error_rate" => Ok(Self::ErrorRate),
            _ => Err(TraceMetricParseError::Unknown),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum TraceMetricParseError {
    #[error("unknown trace metric")]
    Unknown,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "aggr", rename_all = "snake_case")]
pub enum TraceAggr {
    Count {
        interval: Interval,
        object: TraceObject<NoCombine>,
    },
    Mean {
        interval: Interval,
        object: TraceObject<NoCombine>,
    },
    Ci {
        interval: Interval,
        object: TraceObject<NoCombine>,
    },
    Score {
        immediate_interval: ImmediateInterval,
        reference_interval: ReferenceInterval,
        object: TraceObject<CombineScores>,
    },
}

impl TraceAggr {
    fn kind(&self) -> TraceAggrKind {
        match self {
            TraceAggr::Count { .. } => TraceAggrKind::Count,
            TraceAggr::Mean { .. } => TraceAggrKind::Mean,
            TraceAggr::Ci { .. } => TraceAggrKind::Ci,
            TraceAggr::Score { .. } => TraceAggrKind::Score,
        }
    }
}

#[derive(SerializeDisplay, DeserializeFromStr, Debug)]
pub enum TraceAggrKind {
    Count,
    Mean,
    Ci,
    Score,
}

impl Display for TraceAggrKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceAggrKind::Count => write!(f, "count"),
            TraceAggrKind::Mean => write!(f, "mean"),
            TraceAggrKind::Ci => write!(f, "ci"),
            TraceAggrKind::Score => write!(f, "score"),
        }
    }
}

impl FromStr for TraceAggrKind {
    type Err = TraceAggrKindParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "count" => Ok(Self::Count),
            "mean" => Ok(Self::Mean),
            "ci" => Ok(Self::Ci),
            "score" => Ok(Self::Score),
            _ => Err(TraceAggrKindParseError::Unknown),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum TraceAggrKindParseError {
    #[error("unknown trace aggregation")]
    Unknown,
}

const fn metric_name(metric: TraceMetric, aggr: TraceAggrKind) -> MetricName {
    macro_rules! metrics {
        ($metric:ident, $var:ident, $expr:expr) => {
            match $metric {
                TraceMetric::Duration => {
                    const $var: &str = "duration";
                    $expr
                }
                TraceMetric::Busy => {
                    const $var: &str = "busy";
                    $expr
                }
                TraceMetric::CallRate => {
                    const $var: &str = "call_rate";
                    $expr
                }
                TraceMetric::ErrorRate => {
                    const $var: &str = "error_rate";
                    $expr
                }
            }
        };
    }

    macro_rules! aggrs {
        ($aggr:ident, $var:ident, $expr:expr) => {
            match $aggr {
                TraceAggrKind::Count => {
                    const $var: &str = "count";
                    $expr
                }
                TraceAggrKind::Mean => {
                    const $var: &str = "mean";
                    $expr
                }
                TraceAggrKind::Ci => {
                    const $var: &str = "ci";
                    $expr
                }
                TraceAggrKind::Score => {
                    const $var: &str = "score";
                    $expr
                }
            }
        };
    }

    MetricName::new_static(metrics!(
        metric,
        METRIC,
        aggrs!(aggr, AGGR, formatcp!("trace_{METRIC}_{AGGR}"))
    ))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceObject<C>(OperationOrService<TraceOperation, Combine<TraceService, C>>);

type TraceOperation =
    SingleOrMultiple<ItemOrRelation<OperationKey>, ItemOrRelation<OperationFilter>>;
type TraceService = SingleOrMultiple<ItemOrRelation<ServiceKey>, ItemOrRelation<ServiceFilter>>;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationOrService<O, S> {
    Operation(O),
    Service(S),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(tag = "multiplicity", rename_all = "snake_case")]
pub enum SingleOrMultiple<K, F> {
    Single(K),
    Multiple { filter: F, top: Option<u64> },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemOrRelation<K> {
    Item(K),
    Relation {
        #[serde(flatten, with = "prefix_child")]
        child: K,
        #[serde(flatten, with = "prefix_parent")]
        parent: K,
    },
}

with_prefix!(prefix_child "child_");
with_prefix!(prefix_parent "parent_");

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct ServiceKey {
    service_name: String,
    namespace: Option<String>,
    instance_id: Option<String>,
}

impl ServiceKey {
    pub fn new<T: Into<String>>(service_name: T) -> Self {
        Self {
            service_name: service_name.into(),
            namespace: None,
            instance_id: None,
        }
    }

    pub fn namespace<T: Into<String>>(self, namespace: T) -> Self {
        self.opt_namespace(Some(namespace))
    }

    pub fn opt_namespace<T: Into<String>>(mut self, namespace: Option<T>) -> Self {
        self.namespace = namespace.map(|s| s.into());
        self
    }

    pub fn instance_id<T: Into<String>>(self, instance_id: T) -> Self {
        self.opt_instance_id(Some(instance_id))
    }

    pub fn opt_instance_id<T: Into<String>>(mut self, instance_id: Option<T>) -> Self {
        self.instance_id = instance_id.map(|s| s.into());
        self
    }

    pub fn into_filter(self) -> ServiceFilter {
        ServiceFilter {
            service_name: Some(self.service_name),
            namespace: self.namespace,
            instance_id: self.instance_id,
        }
    }

    pub fn labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        std::iter::once((
            LabelName::new_static("service_name"),
            LabelSelector::Eq(self.service_name.to_string()),
        ))
        .chain(self.namespace.as_ref().map(|namespace| {
            (
                LabelName::new_static("service_namespace"),
                LabelSelector::Eq(namespace.to_string()),
            )
        }))
        .chain(self.instance_id.as_ref().map(|instance_id| {
            (
                LabelName::new_static("service_instance_id"),
                LabelSelector::Eq(instance_id.to_string()),
            )
        }))
    }

    pub fn parent_labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        std::iter::once((
            LabelName::new_static("parent_service_name"),
            LabelSelector::Eq(self.service_name.to_string()),
        ))
        .chain(self.namespace.as_ref().map(|namespace| {
            (
                LabelName::new_static("parent_service_namespace"),
                LabelSelector::Eq(namespace.to_string()),
            )
        }))
        .chain(self.instance_id.as_ref().map(|instance_id| {
            (
                LabelName::new_static("parent_service_instance_id"),
                LabelSelector::Eq(instance_id.to_string()),
            )
        }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Default, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct ServiceFilter {
    service_name: Option<String>,
    namespace: Option<String>,
    instance_id: Option<String>,
}

impl ServiceFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn service_name<T: Into<String>>(self, service_name: T) -> Self {
        self.opt_service_name(Some(service_name))
    }

    pub fn opt_service_name<T: Into<String>>(mut self, service_name: Option<T>) -> Self {
        self.service_name = service_name.map(|s| s.into());
        self
    }

    pub fn namespace<T: Into<String>>(self, namespace: T) -> Self {
        self.opt_namespace(Some(namespace))
    }

    pub fn opt_namespace<T: Into<String>>(mut self, namespace: Option<T>) -> Self {
        self.namespace = namespace.map(|s| s.into());
        self
    }

    pub fn instance_id<T: Into<String>>(self, instance_id: T) -> Self {
        self.opt_instance_id(Some(instance_id))
    }

    pub fn opt_instance_id<T: Into<String>>(mut self, instance_id: Option<T>) -> Self {
        self.instance_id = instance_id.map(|s| s.into());
        self
    }

    pub fn labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        self.service_name
            .as_ref()
            .map(|name| {
                (
                    LabelName::new_static("service_name"),
                    LabelSelector::Eq(name.to_string()),
                )
            })
            .into_iter()
            .chain(self.namespace.as_ref().map(|namespace| {
                (
                    LabelName::new_static("service_namespace"),
                    LabelSelector::Eq(namespace.to_string()),
                )
            }))
            .chain(self.instance_id.as_ref().map(|instance_id| {
                (
                    LabelName::new_static("service_instance_id"),
                    LabelSelector::Eq(instance_id.to_string()),
                )
            }))
    }

    pub fn parent_labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        self.service_name
            .as_ref()
            .map(|name| {
                (
                    LabelName::new_static("parent_service_name"),
                    LabelSelector::Eq(name.to_string()),
                )
            })
            .into_iter()
            .chain(self.namespace.as_ref().map(|namespace| {
                (
                    LabelName::new_static("parent_service_namespace"),
                    LabelSelector::Eq(namespace.to_string()),
                )
            }))
            .chain(self.instance_id.as_ref().map(|instance_id| {
                (
                    LabelName::new_static("parent_service_instance_id"),
                    LabelSelector::Eq(instance_id.to_string()),
                )
            }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct OperationKey {
    #[serde(flatten)]
    service: ServiceKey,
    operation_name: String,
}

impl OperationKey {
    pub fn new<T: Into<String>>(service: ServiceKey, operation_name: T) -> Self {
        Self {
            service,
            operation_name: operation_name.into(),
        }
    }

    pub fn into_filter(self) -> OperationFilter {
        OperationFilter {
            service: self.service.into_filter(),
            operation_name: Some(self.operation_name),
        }
    }

    fn labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        self.service.labels().chain(std::iter::once((
            LabelName::new_static("operation_name"),
            LabelSelector::Eq(self.operation_name.to_string()),
        )))
    }

    fn parent_labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        self.service.parent_labels().chain(std::iter::once((
            LabelName::new_static("parent_operation_name"),
            LabelSelector::Eq(self.operation_name.to_string()),
        )))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Default, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct OperationFilter {
    #[serde(flatten)]
    service: ServiceFilter,
    operation_name: Option<String>,
}

impl OperationFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn service(mut self, service: ServiceFilter) -> Self {
        self.service = service;
        self
    }

    pub fn operation_name<T: Into<String>>(self, operation_name: T) -> Self {
        self.opt_operation_name(Some(operation_name))
    }

    pub fn opt_operation_name<T: Into<String>>(mut self, operation_name: Option<T>) -> Self {
        self.operation_name = operation_name.map(|s| s.into());
        self
    }

    fn labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        self.service
            .labels()
            .chain(self.operation_name.as_ref().map(|operation_name| {
                (
                    LabelName::new_static("operation_name"),
                    LabelSelector::Eq(operation_name.to_string()),
                )
            }))
    }

    fn parent_labels(&self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        self.service
            .parent_labels()
            .chain(self.operation_name.as_ref().map(|operation_name| {
                (
                    LabelName::new_static("parent_operation_name"),
                    LabelSelector::Eq(operation_name.to_string()),
                )
            }))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Combine<T, C> {
    #[serde(flatten)]
    value: T,
    #[serde(flatten)]
    combine: C,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct CombineScores {
    combine: CombinationFactor,
}

impl CombineScores {
    pub fn new(combine: CombinationFactor) -> Self {
        Self { combine }
    }
}

// Do not allow combining series.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum NoCombine {}

// Number between 0 and 1?.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct CombinationFactor(#[cfg_attr(feature = "schemars", schemars(with = "f64"))] NotNan<f64>);

impl CombinationFactor {
    pub fn new(factor: NotNan<f64>) -> Self {
        Self(factor)
    }

    pub fn into_inner(self) -> NotNan<f64> {
        self.0
    }

    pub fn into_f64(self) -> f64 {
        self.0.into_inner()
    }
}

impl Default for CombinationFactor {
    fn default() -> Self {
        Self::new(NotNan::new(0.5).unwrap())
    }
}

impl TraceExpr {
    pub fn new(metric: TraceMetric, aggr: TraceAggr) -> Self {
        Self { metric, aggr }
    }

    pub fn expr<P: PromSelect>(&self, params: &P) -> Expr {
        self.aggr.expr(self.metric, params)
    }
}

impl TraceAggr {
    pub fn count<T: Into<Interval>>(interval: T, object: TraceObject<NoCombine>) -> Self {
        Self::Count {
            interval: interval.into(),
            object,
        }
    }

    pub fn mean<T: Into<Interval>>(interval: T, object: TraceObject<NoCombine>) -> Self {
        Self::Mean {
            interval: interval.into(),
            object,
        }
    }

    pub fn ci<T: Into<Interval>>(interval: T, object: TraceObject<NoCombine>) -> Self {
        Self::Ci {
            interval: interval.into(),
            object,
        }
    }

    pub fn score(
        immediate_interval: ImmediateInterval,
        reference_interval: ReferenceInterval,
        object: TraceObject<CombineScores>,
    ) -> Self {
        Self::Score {
            immediate_interval,
            reference_interval,
            object,
        }
    }

    pub fn expr<P: PromSelect>(&self, metric: TraceMetric, params: &P) -> Expr {
        match self {
            TraceAggr::Count { interval, object }
            | TraceAggr::Mean { interval, object }
            | TraceAggr::Ci { interval, object } => {
                let ms = object
                    .metric(metric_name(metric, self.kind()))
                    .labels(interval.labels());
                let expr = Expr::metric(ms);
                match object.top() {
                    Some(n) => params.select(&SelectItem::Top { n }, expr),
                    None => expr,
                }
            }
            TraceAggr::Score {
                immediate_interval,
                reference_interval,
                object,
            } => {
                let ms = object
                    .metric(metric_name(metric, self.kind()))
                    .label(
                        LabelName::new_static("metric_type"),
                        LabelSelector::Eq(String::from("anomaly_score")),
                    )
                    .labels(immediate_interval.labels())
                    .labels(reference_interval.labels());
                let expr = match object.combine() {
                    Some(CombineScores {
                        combine: CombinationFactor(c),
                    }) => {
                        let expr = Expr::metric(ms);
                        let counts = Expr::metric(
                            object
                                .metric(metric_name(metric, TraceAggrKind::Count))
                                .label(
                                    LabelName::new_static("metric_type"),
                                    LabelSelector::Eq(String::from("anomaly_score")),
                                )
                                .label(
                                    LabelName::new_static("immediate"),
                                    LabelSelector::Eq(immediate_interval.to_string()),
                                ),
                        );
                        let labels = Vec::from_iter([
                            LabelName::new_static("service_name"),
                            LabelName::new_static("service_namespace"),
                            LabelName::new_static("service_instance_id"),
                        ]);
                        (expr - 1.0)
                            .clamp_min(0.0)
                            .is_ge(0.0)
                            .sum_by(labels.clone())
                            / counts.sum_by(labels).clamp_min(1.0).pow(c.into_inner())
                            + 1.0
                    }
                    None => Expr::metric(ms).clamp_min(1.0),
                };
                match object.top() {
                    Some(n) => params.select(&SelectItem::Top { n }, expr),
                    None => expr,
                }
            }
        }
    }
}

impl<C> TraceObject<C> {
    pub fn builder() -> TraceObjectBuilder<WantsOperationOrService<C>> {
        TraceObjectBuilder(WantsOperationOrService(PhantomData))
    }

    fn metric(&self, name: MetricName) -> MetricSelector {
        let metric = MetricSelector::new().metric(name).label(
            LabelName::new_static("metric_type"),
            LabelSelector::Eq(String::from("anomaly_score")),
        );
        match &self.0 {
            OperationOrService::Operation(v) => match v {
                SingleOrMultiple::Single(v) => match v {
                    ItemOrRelation::Item(key) => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("default")),
                        )
                        .labels(key.labels()),
                    ItemOrRelation::Relation { child, parent } => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("operation-relations")),
                        )
                        .labels(child.labels())
                        .labels(parent.parent_labels()),
                },
                SingleOrMultiple::Multiple { filter, .. } => match filter {
                    ItemOrRelation::Item(filter) => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("default")),
                        )
                        .labels(filter.labels()),
                    ItemOrRelation::Relation { child, parent } => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("operation-relations")),
                        )
                        .labels(child.labels())
                        .labels(parent.parent_labels()),
                },
            },
            OperationOrService::Service(Combine { value, .. }) => match value {
                SingleOrMultiple::Single(v) => match v {
                    ItemOrRelation::Item(key) => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("default")),
                        )
                        .labels(key.labels()),
                    ItemOrRelation::Relation { child, parent } => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("operation-relations")),
                        )
                        .labels(child.labels())
                        .labels(parent.parent_labels()),
                },
                SingleOrMultiple::Multiple { filter, .. } => match filter {
                    ItemOrRelation::Item(key) => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("default")),
                        )
                        .labels(key.labels()),
                    ItemOrRelation::Relation { child, parent } => metric
                        .label(
                            LabelName::new_static("config"),
                            LabelSelector::Eq(String::from("operation-relations")),
                        )
                        .labels(child.labels())
                        .labels(parent.parent_labels()),
                },
            },
        }
    }

    fn top(&self) -> Option<u64> {
        match &self.0 {
            OperationOrService::Operation(SingleOrMultiple::Multiple { top, .. })
            | OperationOrService::Service(Combine {
                value: SingleOrMultiple::Multiple { top, .. },
                ..
            }) => *top,
            _ => None,
        }
    }

    fn combine(&self) -> Option<&C> {
        match &self.0 {
            OperationOrService::Service(Combine { combine, .. }) => Some(combine),
            _ => None,
        }
    }
}

pub struct TraceObjectBuilder<T>(T);
pub struct WantsOperationOrService<C>(PhantomData<C>);
pub struct WantsSingleOrMultiple<T, C>(T, PhantomData<C>);
pub struct WantsItemOrRelation<T, S, C>(T, S, PhantomData<C>);

pub struct Operation(());
pub struct Service<C> {
    combine: C,
}

pub struct Single(());
pub struct Multiple {
    top: Option<u64>,
}

impl<C> TraceObjectBuilder<WantsOperationOrService<C>> {
    pub fn operation(self) -> TraceObjectBuilder<WantsSingleOrMultiple<Operation, C>> {
        TraceObjectBuilder(WantsSingleOrMultiple(Operation(()), PhantomData))
    }
    pub fn service(self, combine: C) -> TraceObjectBuilder<WantsSingleOrMultiple<Service<C>, C>> {
        TraceObjectBuilder(WantsSingleOrMultiple(Service { combine }, PhantomData))
    }
}

impl<T, C> TraceObjectBuilder<WantsSingleOrMultiple<T, C>> {
    pub fn single(self) -> TraceObjectBuilder<WantsItemOrRelation<T, Single, C>> {
        TraceObjectBuilder(WantsItemOrRelation(self.0 .0, Single(()), PhantomData))
    }
    pub fn multiple(
        self,
        top: Option<u64>,
    ) -> TraceObjectBuilder<WantsItemOrRelation<T, Multiple, C>> {
        TraceObjectBuilder(WantsItemOrRelation(
            self.0 .0,
            Multiple { top },
            PhantomData,
        ))
    }
}

pub trait Build<A, B> {
    fn build(self, arg: A) -> B;
}

pub trait IsOperationOrService<C>:
    Build<
    SingleOrMultiple<ItemOrRelation<Self::Key>, ItemOrRelation<Self::Filter>>,
    OperationOrService<TraceOperation, Combine<TraceService, C>>,
>
{
    type Key;
    type Filter;
}

impl<C> IsOperationOrService<C> for Operation {
    type Key = OperationKey;
    type Filter = OperationFilter;
}

impl<O, S> Build<O, OperationOrService<O, S>> for Operation {
    fn build(self, value: O) -> OperationOrService<O, S> {
        OperationOrService::Operation(value)
    }
}

impl<C> IsOperationOrService<C> for Service<C> {
    type Key = ServiceKey;
    type Filter = ServiceFilter;
}

impl<O, S, C> Build<S, OperationOrService<O, Combine<S, C>>> for Service<C> {
    fn build(self, value: S) -> OperationOrService<O, Combine<S, C>> {
        OperationOrService::Service(Combine {
            value,
            combine: self.combine,
        })
    }
}

pub trait IsSingleOrMultiple<T: IsOperationOrService<C>, C>:
    Build<
    ItemOrRelation<Self::Key>,
    SingleOrMultiple<ItemOrRelation<T::Key>, ItemOrRelation<T::Filter>>,
>
{
    type Key;
}

impl<T: IsOperationOrService<C>, C> IsSingleOrMultiple<T, C> for Single {
    type Key = T::Key;
}

impl<K, F> Build<K, SingleOrMultiple<K, F>> for Single {
    fn build(self, value: K) -> SingleOrMultiple<K, F> {
        SingleOrMultiple::Single(value)
    }
}

impl<T: IsOperationOrService<C>, C> IsSingleOrMultiple<T, C> for Multiple {
    type Key = T::Filter;
}

impl<K, F> Build<F, SingleOrMultiple<K, F>> for Multiple {
    fn build(self, filter: F) -> SingleOrMultiple<K, F> {
        SingleOrMultiple::Multiple {
            filter,
            top: self.top,
        }
    }
}

impl<T: IsOperationOrService<C>, S: IsSingleOrMultiple<T, C>, C>
    TraceObjectBuilder<WantsItemOrRelation<T, S, C>>
{
    pub fn item(self, key: S::Key) -> TraceObject<C> {
        self.build(ItemOrRelation::Item(key))
    }
    pub fn relation(self, child: S::Key, parent: S::Key) -> TraceObject<C> {
        self.build(ItemOrRelation::Relation { child, parent })
    }

    fn build(self, item_or_relation: ItemOrRelation<S::Key>) -> TraceObject<C> {
        let single_or_multiple = self.0 .1.build(item_or_relation);
        let operation_or_service = self.0 .0.build(single_or_multiple);
        TraceObject(operation_or_service)
    }
}

#[cfg(test)]
mod test {
    use ordered_float::NotNan;
    use prometheus_api::InstantQueryParams;

    use crate::{
        exprs::precalculated::{CombinationFactor, CombineScores},
        ImmediateInterval, ReferenceInterval, ServiceFilter, TraceAggr, TraceExpr, TraceMetric,
    };

    use super::{NoCombine, OperationKey, ServiceKey, TraceObject};

    #[test]
    fn build_trace_object() {
        let _example = TraceObject::<NoCombine>::builder()
            .operation()
            .single()
            .item(OperationKey::new(
                ServiceKey::new("relation-graph-engine")
                    .namespace("continuousc")
                    .instance_id("demo"),
                "POST",
            ));
    }

    #[test]
    fn serialize_single_operation_trace_object() {
        let example = TraceObject::<NoCombine>::builder()
            .operation()
            .single()
            .item(OperationKey::new(
                ServiceKey::new("relation-graph-engine")
                    .namespace("continuousc")
                    .instance_id("demo"),
                "POST",
            ));
        let s = serde_json::to_string(&example).unwrap();
        assert_eq!(
            s,
            r#"{"type":"operation","multiplicity":"single","kind":"item","service_name":"relation-graph-engine","namespace":"continuousc","instance_id":"demo","operation_name":"POST"}"#
        );
    }

    #[test]
    fn serialize_single_combined_service_trace_object() {
        let example = TraceObject::<CombineScores>::builder()
            .service(CombineScores {
                combine: CombinationFactor(NotNan::new(0.5).unwrap()),
            })
            .single()
            .item(
                ServiceKey::new("relation-graph-engine")
                    .namespace("continuousc")
                    .instance_id("demo"),
            );
        let s = serde_json::to_string(&example).unwrap();
        assert_eq!(
            s,
            r#"{"type":"service","multiplicity":"single","kind":"item","service_name":"relation-graph-engine","namespace":"continuousc","instance_id":"demo","combine":0.5}"#
        );
    }

    #[test]
    fn serialize_single_combined_service_relation_trace_object() {
        let example = TraceObject::<CombineScores>::builder()
            .service(CombineScores {
                combine: CombinationFactor(NotNan::new(0.5).unwrap()),
            })
            .single()
            .relation(
                ServiceKey::new("relation-graph-engine")
                    .namespace("continuousc")
                    .instance_id("demo"),
                ServiceKey::new("frontend")
                    .namespace("continuousc")
                    .instance_id("demo"),
            );
        let s = serde_json::to_string(&example).unwrap();
        assert_eq!(
            s,
            r#"{"type":"service","multiplicity":"single","kind":"relation","child_service_name":"relation-graph-engine","child_namespace":"continuousc","child_instance_id":"demo","parent_service_name":"frontend","parent_namespace":"continuousc","parent_instance_id":"demo","combine":0.5}"#
        );
    }

    #[test]
    fn combined_score_expr() {
        let expr = TraceExpr::new(
            TraceMetric::Duration,
            TraceAggr::score(
                ImmediateInterval::I15m,
                ReferenceInterval::R30d,
                TraceObject::builder()
                    .service(CombineScores::new(CombinationFactor::new(
                        NotNan::new(0.5).unwrap(),
                    )))
                    .multiple(Some(5))
                    .item(ServiceFilter::new()),
            ),
        );
        let params = InstantQueryParams { time: None };
        assert_eq!(
            expr.expr(&params).to_string(),
            r#"topk(5, sum by (service_name, service_namespace, service_instance_id) (clamp_min(trace_duration_score { config = "default", immediate = "15m", metric_type = "anomaly_score", reference = "30d" } - 1, 0) >= 0) / clamp_min(sum by (service_name, service_namespace, service_instance_id) (trace_duration_count { config = "default", immediate = "15m", metric_type = "anomaly_score" }), 1) ^ 0.5 + 1)"#
        );
    }
}
