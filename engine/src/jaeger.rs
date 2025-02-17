/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{convert::Infallible, fmt::Display, num::ParseIntError, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct TraceId(String);

impl Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TraceId {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TraceId(s.to_string()))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct SpanId(String);

impl Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct ServiceNamespace(pub String);

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct ServiceName(pub String);

impl Display for ServiceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct ServiceInstanceId(pub String);

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct OperationName(pub String);

impl Display for OperationName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    #[serde(rename = "traceID")]
    pub trace_id: TraceId,
    #[serde(rename = "spanID")]
    pub span_id: SpanId,
    pub operation_name: OperationName,
    pub references: Vec<Reference>,
    pub start_time: i64,
    pub start_time_millis: i64,
    pub duration: i64,
    pub tags: Vec<Tag>,
    pub logs: Vec<Log>,
    pub process: Process,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub ref_type: RefType,
    #[serde(rename = "traceID")]
    pub trace_id: TraceId,
    #[serde(rename = "spanID")]
    pub span_id: SpanId,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefType {
    ChildOf,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub service_name: ServiceName,
    pub tags: Vec<Tag>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub key: String,
    #[serde(flatten)]
    pub value: TagValue, // pub r#type: TagType,
                         // pub value: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Log {}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum TagType {
    String,
    Int64,
    Bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum TagValue {
    String(String),
    Int64(Int64),
    Bool(Bool),
}

#[derive(PartialEq, Eq)]
pub enum TagValueRef<'a> {
    String(&'a str),
    Int64(i64),
    Bool(bool),
}

impl TagValue {
    pub fn as_ref(&self) -> TagValueRef<'_> {
        match self {
            TagValue::String(s) => TagValueRef::String(s.as_str()),
            TagValue::Int64(v) => TagValueRef::Int64(v.0),
            TagValue::Bool(v) => TagValueRef::Bool(matches!(v, Bool::True)),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            TagValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            TagValue::Int64(v) => Some(v.0),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TagValue::Bool(v) => Some(matches!(v, Bool::True)),
            _ => None,
        }
    }
}

impl TagValueRef<'_> {
    pub fn to_owned(&self) -> TagValue {
        match self {
            Self::String(s) => TagValue::String(s.to_string()),
            Self::Int64(n) => TagValue::Int64(Int64(*n)),
            Self::Bool(v) => TagValue::Bool(match v {
                true => Bool::True,
                false => Bool::False,
            }),
        }
    }
}

#[derive(
    SerializeDisplay, DeserializeFromStr, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug,
)]
pub struct Int64(pub i64);

impl Display for Int64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Int64 {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Bool {
    True,
    False,
}
