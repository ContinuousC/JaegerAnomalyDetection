/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

mod accum;
pub mod config;
mod error;
// mod graph;
mod jaeger;
pub mod metrics;
mod opensearch;
mod processor;
mod schema;
pub mod state;
mod web;
mod welford;
mod window;

use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use opensearch::EsKeepAlive;
use processor::proc::Processor;

use error::{Error, Result};
use url::Url;
use web::{run_web_server, web_server_spec, AppData};

#[derive(Parser, Clone)]
struct Args {
    #[clap(long, env, default_value = "ca.crt")]
    opensearch_ca: PathBuf,
    #[clap(long, env, default_value = "tls.crt")]
    opensearch_cert: PathBuf,
    #[clap(long, env, default_value = "tls.key")]
    opensearch_key: PathBuf,
    #[clap(long, env, default_value = "https://localhost:9200/")]
    opensearch_url: Url,
    #[clap(long, env)]
    opensearch_user: Option<String>,
    #[clap(long, env, requires = "opensearch_user")]
    opensearch_password: Option<String>,
    #[clap(long, env, default_value = "https://localhost:8080/")]
    prometheus_url: Url,
    #[clap(long, env)]
    prometheus_tenant: Option<String>,
    #[clap(long, env, default_value = "state.cbor")]
    state: PathBuf,
    #[clap(long, env, default_value = "10000")]
    metrics_per_request: usize,
    #[clap(long, env, default_value = "/api/jaeger-anomaly-detection")]
    prefix: String,
    #[clap(long, env, default_value = "127.0.0.1:9999")]
    bind: String,
    #[clap(long)]
    spec: bool,
}

const INDEX: &str = "jaeger-span-*";
const KEEP_ALIVE: EsKeepAlive = EsKeepAlive::Minutes(5);

// Max number of roots to retrieve per query.
const BATCH_SIZE: usize = 1000;
// Max number of traces to retrieve at once.
const CHUNK_SIZE: usize = 50;
// Max number of spans per batch.
const MAX_SPANS: usize = 1000;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    env_logger::init();

    if let Err(e) = run(&args).await {
        log::error!("{e}");
        std::process::exit(1);
    }
}

async fn run(args: &Args) -> Result<()> {
    if args.spec {
        let spec = web_server_spec(args);
        println!("{}", serde_json::to_string_pretty(&spec).unwrap());
        return Ok(());
    }

    let processor = Arc::new(Processor::new(args).await?);
    run_web_server(
        args,
        AppData {
            processor: processor.clone(),
        },
    )
    .await?;

    if let Err(e) = Arc::try_unwrap(processor)
        .map_err(|_| Error::ProcessorShutdown)?
        .shutdown()
        .await
    {
        log::warn!("processor task failed: {e}")
    }

    Ok(())
}
