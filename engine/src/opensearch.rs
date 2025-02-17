/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::fmt::Display;

use serde::{ser::SerializeMap, Deserialize, Serialize};
use serde_with::SerializeDisplay;

use crate::error::{Error, Result};

/* Result and Error */

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum EsResponse<T> {
    Ok(T),
    Err(EsError),
    // #[serde(skip)]
    Unrecognized(serde_json::Value),
}

#[derive(Deserialize, Debug)]
pub struct EsError {
    pub status: u32,
    pub error: EsErrorMsg,
}

#[derive(Deserialize, Debug)]
pub struct EsErrorMsg {
    // pub phase: Option<String>,
    // pub grouped: Option<bool>,
    // pub failed_shards: Option<Vec<EsFailedShard>>,
    //pub root_cause: Vec<EsReason>,
    #[serde(flatten)]
    pub reason: EsReason,
}

#[derive(Deserialize)]
pub struct EsFailedShard {
    // pub shard: u32,
    // pub index: String,
    // pub node: String,
    // pub reason: EsReason,
}

#[derive(Deserialize, Debug)]
pub struct EsReason {
    pub r#type: String,
    pub reason: String,
    // pub caused_by: Option<Box<EsReason>>,
}

impl<T> EsResponse<T> {
    pub fn into_result(self) -> Result<T> {
        match self {
            Self::Ok(v) => Ok(v),
            Self::Err(e) => Err(Error::ElasticErr(e)),
            Self::Unrecognized(v) => Err(Error::ElasticUnknown(v)),
        }
    }
}

impl Display for EsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "status #{}: {}", self.status, self.error)
    }
}

impl Display for EsErrorMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl Display for EsReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.r#type, self.reason)
    }
}

/* Query result */

#[derive(Serialize)]
pub struct EsSearchRequest<T, S> {
    pub query: T,
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pit: Option<EsPit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<EsSortField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_after: Option<S>,
}

pub struct EsSortField {
    pub field: String,
    pub opts: EsSortOpts,
}

impl Serialize for EsSortField {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&self.field, &self.opts)?;
        map.end()
    }
}

#[derive(Serialize)]
pub struct EsSortOpts {
    pub order: EsSortOrder,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EsSortOrder {
    Asc,
    // Desc,
}

#[derive(Deserialize, Debug)]
pub struct EsSearchResponse<T, S> {
    // pub took: u64,
    // pub timed_out: bool,
    // #[serde(rename = "_shards")]
    // pub shards: EsShards,
    #[serde(default)]
    pub pit_id: Option<EsPitId>,
    pub hits: EsHits<T, S>,
}

#[derive(Deserialize, Debug)]
pub struct EsShards {
    // pub total: u64,
    // pub successful: u64,
    // pub skipped: u64,
    // pub failed: u64,
}

#[derive(Deserialize, Debug)]
pub struct EsHits<T, S> {
    pub total: EsTotal,
    // pub max_score: Option<f64>,
    pub hits: Vec<EsHit<T, S>>,
}

#[derive(Deserialize, Debug)]
pub struct EsTotal {
    pub relation: EsRel,
    // pub value: u64,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EsRel {
    Eq,
    Gte,
    Lte,
}

#[derive(Deserialize, Debug)]
pub struct EsHit<T, S> {
    // #[serde(rename = "_index")]
    // pub index: String,
    // #[serde(rename = "_id")]
    // pub id: String,
    // #[serde(rename = "_score")]
    // pub score: Option<f64>,
    #[serde(rename = "_source")]
    pub source: T,
    #[serde(default = "default_sort")]
    pub sort: Option<S>,
}

fn default_sort<S>() -> Option<S> {
    None
}

/* Point-in-time */

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EsPitId(String);

#[derive(Serialize)]
pub struct EsPit {
    pub id: EsPitId,
    pub keep_alive: EsKeepAlive,
}

#[derive(SerializeDisplay, Clone, Copy, Debug)]
pub enum EsKeepAlive {
    Minutes(u64),
}

impl Display for EsKeepAlive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EsKeepAlive::Minutes(n) => write!(f, "{n}m"),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct EsCreatePitQuery {
    pub keep_alive: EsKeepAlive,
    pub allow_partial_pit_creation: bool,
}

#[derive(Serialize, Debug)]
pub struct EsDeletePitRequest {
    pub pit_id: EsPitId,
}

#[derive(Deserialize, Debug)]
pub struct EsCreatePitResponse {
    pub pit_id: EsPitId,
}

#[derive(Deserialize, Debug)]
pub struct EsDeletePitResponse {
    // pub pits: Vec<DeletePitAction>,
}

#[derive(Deserialize, Debug)]
pub struct DeletePitAction {
    // pub successful: bool,
    // pub pit_id: String,
}
