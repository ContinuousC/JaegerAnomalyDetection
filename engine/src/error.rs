/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::path::PathBuf;

use crate::opensearch::EsError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to read file: {0}: {1}")]
    ReadFile(PathBuf, std::io::Error),
    #[error("failed to load ca certificate: {0}: {1}")]
    LoadCa(PathBuf, reqwest::Error),
    #[error("failed to load client certificate and key: {0} / {1}: {2}")]
    LoadCert(PathBuf, PathBuf, reqwest::Error),
    #[error("failed to read state: {0}")]
    ReadState(std::io::Error),
    #[error("failed to write state: {0}")]
    WriteState(std::io::Error),
    #[error("failed to deserialize state: {0}")]
    DeserializeState(ciborium::de::Error<std::io::Error>),
    #[error("url parse error: {0}")]
    Url(url::ParseError),
    #[error("opensearch request failed: {0}")]
    Elastic(reqwest::Error),
    #[error("opensearch returned an error: {}: {}",
			.0.error.reason.r#type,
			.0.error.reason.reason)]
    ElasticErr(EsError),
    #[error("opensearch returned an unknown response: {}",
			serde_json::to_string(.0).unwrap())]
    ElasticUnknown(serde_json::Value),
    #[error("opensearch response missing pit id")]
    ElasticMissingPitId,
    #[error("failed to build prometheus remote write request: {0}")]
    BuildPromRequest(Box<dyn std::error::Error + Send + Sync>),
    // #[error("failed to build prometheus remote write reqwest: {0}")]
    // BuildPromReqwest(reqwest::Error),
    #[error("prometheus remote write request failed: {0}")]
    Prometheus(reqwest::Error),
    #[error("invalid prometheus tenant: {0}")]
    InvalidPrometheusTenant(reqwest::header::InvalidHeaderValue),
    #[error("prometheus remote write request failed: {0}")]
    PromRes(String),
    #[error("failed to bind address: {0}: {1}")]
    Bind(String, std::io::Error),
    #[error("web server error: {0}")]
    WebServer(std::io::Error),
    #[error("failed to shutdown processor: still in use")]
    ProcessorShutdown,
    #[error("DateTime error: {0}")]
    DateTimeBounds(chrono::OutOfRangeError),
    #[error("unspecified DateTime error")]
    DateTime,
    #[error("failed to join processor task: {0}")]
    JoinProcessor(tokio::task::JoinError),
}
