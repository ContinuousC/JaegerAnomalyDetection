/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{collections::BTreeSet, fmt::Display, str::FromStr};

use apistos::ApiComponent;
use jaeger_anomaly_detection::{Duration, WindowConfig};
use prometheus_core::LabelName;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

use crate::{
    jaeger::{Span, TagValueRef},
    processor::trace::TraceConfig,
};

#[derive(Serialize, Deserialize, schemars::JsonSchema, ApiComponent, PartialEq, Clone, Debug)]
#[serde(default)]
pub struct Config {
    #[serde(flatten)]
    pub trace: TraceConfig,
    pub query_interval: Duration,
    pub max_history: Duration,
    pub delay: Duration,
}

#[derive(
    Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug,
)]
pub struct ConfigName(String);

impl ConfigName {
    pub fn new<T: Into<String>>(name: T) -> Self {
        Self(name.into())
    }
}

impl Display for ConfigName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(
    Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug,
)]
pub struct MetricName(String);

impl MetricName {
    pub fn new<T: Into<String>>(name: T) -> Self {
        Self(name.into())
    }
}

impl Display for MetricName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SpanSelector {
    All(Vec<SpanSelector>),
    Any(Vec<SpanSelector>),
    Not(Box<SpanSelector>),
    Has(SpanKey),
    In(SpanKey, BTreeSet<String>),
    NotIn(SpanKey, BTreeSet<String>),
    Match(SpanKey, Regex),
    NoMatch(SpanKey, Regex),
    KeyEq(SpanKey, SpanKey),
    KeyNe(SpanKey, SpanKey),
    Eq(SpanKey, i64),
    Ne(SpanKey, i64),
    Inside(SpanKey, Range),
    Outside(SpanKey, Range),
    IsTrue(SpanKey),
    IsFalse(SpanKey),
}

#[derive(SerializeDisplay, DeserializeFromStr, Clone, Debug)]
pub struct Regex(regex::Regex);

impl Regex {
    pub fn new(re: &str) -> Result<Self, regex::Error> {
        Ok(Self(regex::Regex::new(re)?))
    }

    pub fn matches(&self, s: &str) -> bool {
        self.0.is_match(s)
    }
}

impl Display for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Regex {
    type Err = regex::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

impl Eq for Regex {}
impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl schemars::JsonSchema for Regex {
    fn schema_name() -> std::string::String {
        "Regex".to_owned()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(std::concat!(std::module_path!(), "::", "Regex"))
    }
    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        gen.subschema_for::<String>()
    }
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct Range {
    pub lower: Option<LowerBound>,
    #[serde(alias = "higher")]
    pub upper: Option<UpperBound>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LowerBound {
    Gt(i64),
    Ge(i64),
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum UpperBound {
    Lt(i64),
    Le(i64),
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    Tag(String),
    SelfDuration,
    TagExcept {
        tag: String,
        key: String,
    },
    Count {
        window: WindowConfig,
    },
    Rate {
        select: SpanSelector,
        window: WindowConfig,
    },
}

#[derive(
    Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug,
)]
#[serde(rename_all = "snake_case")]
pub enum SpanKey {
    Current(KeyName),
    Parent(KeyName),
}

#[derive(
    Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug,
)]
#[serde(rename_all = "snake_case")]
pub enum KeyName {
    OperationName,
    ServiceName,
    ProcessTag(String),
    SpanTag(String),
    Duration,
}

impl SpanSelector {
    pub(crate) fn matches(&self, span: &Span, parent: Option<&Span>) -> bool {
        match self {
            SpanSelector::All(sels) => sels.iter().all(|sel| sel.matches(span, parent)),
            SpanSelector::Any(sels) => sels.iter().any(|sel| sel.matches(span, parent)),
            SpanSelector::Not(sel) => !sel.matches(span, parent),
            SpanSelector::Has(key) => key.get(span, parent).is_some(),
            SpanSelector::In(key, values) => {
                if let Some(TagValueRef::String(s)) = key.get(span, parent) {
                    values.contains(s)
                } else {
                    false
                }
            }
            SpanSelector::NotIn(key, values) => {
                if let Some(TagValueRef::String(s)) = key.get(span, parent) {
                    !values.contains(s)
                } else {
                    false
                }
            }
            SpanSelector::KeyEq(a, b) => a.get(span, parent) == b.get(span, parent),
            SpanSelector::KeyNe(a, b) => a.get(span, parent) != b.get(span, parent),
            SpanSelector::Eq(key, v) => {
                if let Some(TagValueRef::Int64(n)) = key.get(span, parent) {
                    n == *v
                } else {
                    false
                }
            }
            SpanSelector::Match(key, re) => {
                if let Some(TagValueRef::String(s)) = key.get(span, parent) {
                    re.matches(s)
                } else {
                    false
                }
            }
            SpanSelector::NoMatch(key, re) => {
                if let Some(TagValueRef::String(s)) = key.get(span, parent) {
                    !re.matches(s)
                } else {
                    false
                }
            }
            SpanSelector::Ne(key, v) => {
                if let Some(TagValueRef::Int64(n)) = key.get(span, parent) {
                    n != *v
                } else {
                    false
                }
            }
            SpanSelector::Inside(key, range) => {
                if let Some(TagValueRef::Int64(n)) = key.get(span, parent) {
                    range.contains(n)
                } else {
                    false
                }
            }
            SpanSelector::Outside(key, range) => {
                if let Some(TagValueRef::Int64(n)) = key.get(span, parent) {
                    !range.contains(n)
                } else {
                    false
                }
            }
            SpanSelector::IsTrue(key) => {
                if let Some(TagValueRef::Bool(v)) = key.get(span, parent) {
                    v
                } else {
                    false
                }
            }
            SpanSelector::IsFalse(key) => {
                if let Some(TagValueRef::Bool(v)) = key.get(span, parent) {
                    !v
                } else {
                    false
                }
            }
        }
    }
}

impl SpanKey {
    pub fn get<'a>(&self, span: &'a Span, parent: Option<&'a Span>) -> Option<TagValueRef<'a>> {
        match self {
            SpanKey::Current(key) => key.get(span),
            SpanKey::Parent(key) => parent.and_then(|span| key.get(span)),
        }
    }

    pub fn label(&self) -> LabelName {
        match self {
            SpanKey::Current(key) => key.label(),
            SpanKey::Parent(key) => LabelName::new(format!("parent_{}", key.label())).unwrap(),
        }
    }

    pub fn is_required(&self) -> bool {
        match self {
            SpanKey::Current(key) => key.is_required(),
            SpanKey::Parent(_) => false,
        }
    }
}

impl KeyName {
    pub fn get<'a>(&self, span: &'a Span) -> Option<TagValueRef<'a>> {
        match self {
            KeyName::OperationName => Some(TagValueRef::String(span.operation_name.0.as_str())),
            KeyName::ServiceName => Some(TagValueRef::String(span.process.service_name.0.as_str())),
            KeyName::Duration => Some(TagValueRef::Int64(span.duration)),
            KeyName::ProcessTag(name) => span
                .process
                .tags
                .iter()
                .find(|tag| &tag.key == name)
                .map(|tag| tag.value.as_ref()),
            KeyName::SpanTag(name) => span
                .tags
                .iter()
                .find(|tag| &tag.key == name)
                .map(|tag| tag.value.as_ref()),
        }
    }

    pub fn label(&self) -> LabelName {
        match self {
            KeyName::OperationName => LabelName::new("operation_name").unwrap(),
            KeyName::ServiceName => LabelName::new("service_name").unwrap(),
            KeyName::ProcessTag(tag) | KeyName::SpanTag(tag) => LabelName::new(
                tag.chars()
                    .skip_while(|c| !c.is_ascii_alphabetic())
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect::<String>(),
            )
            .unwrap(),
            KeyName::Duration => LabelName::new("duration").unwrap(),
        }
    }

    pub fn is_required(&self) -> bool {
        match self {
            KeyName::OperationName | KeyName::ServiceName | KeyName::Duration => true,
            KeyName::ProcessTag(_) | KeyName::SpanTag(_) => false,
        }
    }
}

impl Range {
    fn contains(&self, n: i64) -> bool {
        self.lower.as_ref().map_or(true, |bound| bound.matches(n))
            && self.upper.as_ref().map_or(true, |bound| bound.matches(n))
    }
}

impl LowerBound {
    fn matches(&self, n: i64) -> bool {
        match self {
            LowerBound::Gt(b) => n > *b,
            LowerBound::Ge(b) => n >= *b,
        }
    }
}

impl UpperBound {
    fn matches(&self, n: i64) -> bool {
        match self {
            UpperBound::Lt(b) => n < *b,
            UpperBound::Le(b) => n <= *b,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            trace: TraceConfig::default(),
            query_interval: Duration::Seconds(30),
            max_history: Duration::Hours(1),
            delay: Duration::Minutes(2),
        }
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::{KeyName, LowerBound, Range, Regex, SpanSelector, UpperBound};
    use crate::{config::SpanKey, jaeger::Span};

    #[test]
    fn match_error() {
        let span = serde_json::from_value::<Span>(json!({
        "traceID": "0de61f1de7ee678bccb46f3dab804867",
        "spanID": "672633d1537fb110",
        "operationName": "GET",
        "references": [
          {
            "refType": "CHILD_OF",
            "traceID": "0de61f1de7ee678bccb46f3dab804867",
            "spanID": "ad68c4f3da7c8f3c"
          }
        ],
        "startTime": 1716537605749742i64,
        "startTimeMillis": 1716537605749i64,
        "duration": 1530,
        "tags": [
          {
            "key": "otel.library.name",
            "type": "string",
            "value": "opentelemetry-otlp"
          },
          {
            "key": "otel.library.version",
            "type": "string",
            "value": "0.14.0"
          },
          {
            "key": "code.filepath",
            "type": "string",
            "value": "/root/.cargo/registry/src/index.crates.io-6f17d22bba15001f/reqwest-tracing-0.4.8/src/reqwest_otel_span_builder.rs"
          },
          {
            "key": "code.namespace",
            "type": "string",
            "value": "reqwest_tracing::reqwest_otel_span_builder"
          },
          {
            "key": "code.lineno",
            "type": "int64",
            "value": "138"
          },
          {
            "key": "thread.id",
            "type": "int64",
            "value": "1"
          },
          {
            "key": "thread.name",
            "type": "string",
            "value": "main"
          },
          {
            "key": "http.method",
            "type": "string",
            "value": "GET"
          },
          {
            "key": "http.scheme",
            "type": "string",
            "value": "http"
          },
          {
            "key": "http.host",
            "type": "string",
            "value": "cortex-ruler.cortex"
          },
          {
            "key": "net.host.port",
            "type": "string",
            "value": "8080"
          },
          {
            "key": "http.url",
            "type": "string",
            "value": "http://cortex-ruler.cortex:8080/api/prom/api/v1/alerts"
          },
          {
            "key": "http.status_code",
            "type": "string",
            "value": "200"
          },
          {
            "key": "http.user_agent",
            "type": "string",
            "value": ""
          },
          {
            "key": "busy_ns",
            "type": "int64",
            "value": "80424"
          },
          {
            "key": "idle_ns",
            "type": "int64",
            "value": "1446454"
          },
          {
            "key": "span.kind",
            "type": "string",
            "value": "client"
          },
          {
            "key": "internal.span.format",
            "type": "string",
            "value": "otlp"
          }
        ],
        "logs": [
          {
            "timestamp": 1716537605749779i64,
            "fields": [
              {
                "key": "event",
                "type": "string",
                "value": "reuse idle connection for (\"http\", cortex-ruler.cortex:8080)"
              },
              {
                "key": "level",
                "type": "string",
                "value": "DEBUG"
              },
              {
                "key": "target",
                "type": "string",
                "value": "hyper::client::pool"
              },
              {
                "key": "code.filepath",
                "type": "string",
                "value": "/root/.cargo/registry/src/index.crates.io-6f17d22bba15001f/hyper-0.14.28/src/client/pool.rs"
              },
              {
                "key": "code.namespace",
                "type": "string",
                "value": "hyper::client::pool"
              },
              {
                "key": "code.lineno",
                "type": "int64",
                "value": "254"
              }
            ]
          }
        ],
        "process": {
          "serviceName": "relation-graph-engine",
          "tags": [
            {
              "key": "service.version",
              "type": "string",
              "value": "0.1.5-acc.7"
            },
            {
              "key": "k8s.container.name",
              "type": "string",
              "value": "relation-graph-engine"
            },
            {
              "key": "k8s.node.name",
              "type": "string",
              "value": "k3s-1"
            },
            {
              "key": "k8s.pod.name",
              "type": "string",
              "value": "relation-graph-engine-0"
            },
            {
              "key": "k8s.pod.uid",
              "type": "string",
              "value": "d0bbb715-9469-40de-9728-eff6940d97c9"
            },
            {
              "key": "k8s.namespace.name",
              "type": "string",
              "value": "tenant-mdp"
            },
            {
              "key": "service.namespace",
              "type": "string",
              "value": "continuousc"
            },
            {
              "key": "service.instance.id",
              "type": "string",
              "value": "tenant-mdp"
            }
          ]
        }})).unwrap();

        let selector = SpanSelector::Any(vec![
            SpanSelector::Inside(
                SpanKey::Current(KeyName::SpanTag(String::from("http.status_code"))),
                Range {
                    lower: Some(LowerBound::Ge(200)),
                    upper: Some(UpperBound::Le(299)),
                },
            ),
            SpanSelector::Match(
                SpanKey::Current(KeyName::SpanTag(String::from("http.status_code"))),
                Regex::new("^2..$").unwrap(),
            ),
        ]);

        assert!(selector.matches(&span, None));
    }
}
