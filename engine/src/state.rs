/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    config::{ConfigName, KeyName, MetricName},
    jaeger::TagValue,
    processor::trace::TraceState,
};

use super::config::Config;

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    pub config: Config,
    pub state: TraceState,
    pub last: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessorState {
    pub groups: BTreeMap<ConfigName, SpanProcessorState>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SpanProcessorState {
    pub groups: BTreeMap<BTreeMap<KeyName, TagValue>, BTreeMap<MetricName, MetricProcessorState>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricProcessorState {}
