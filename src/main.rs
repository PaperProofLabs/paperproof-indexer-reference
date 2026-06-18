// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use paperproof_indexer_reference::{
    ApiConfig, ApiState, ReferenceIndexerConfig, build_cursor_store, build_event_sink,
    content::{PaperProofContentEnricher, default_walrus_content_service},
    export_airdrop_snapshot,
    official_content::OfficialContentConfig,
    rebuild_normalized_from_postgres_raw, rebuild_normalized_from_sqlite_raw,
    replay_jsonl_to_state, run_api_server, run_backfill_once, run_tail_once,
    schema::{POSTGRES_REFERENCE_SCHEMA, SQLITE_REFERENCE_SCHEMA},
};
use paperproof_sdk_rs::{IndexerTrustPolicy, PaperProofIndexerClient, PaperProofQueryClient};
#[cfg(feature = "sui-native")]
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(version, about = "Reference indexer for PaperProof Protocol")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Backfill(IndexerArgs),
    CheckpointBackfill(CheckpointArgs),
    Tail(TailArgs),
    Replay(ReplayArgs),
    RebuildNormalized(RebuildNormalizedArgs),
    Airdrop(AirdropArgs),
    CheckDeployment(DeploymentCheckArgs),
    EnrichContent(EnrichArgs),
    Serve(ServeArgs),
    Schema {
        #[arg(long, value_enum, default_value_t = SchemaKind::Sqlite)]
        kind: SchemaKind,
    },
}

#[derive(Debug, Clone, Parser)]
struct ReplayArgs {
    #[arg(long)]
    input: String,
    #[arg(long)]
    output: Option<String>,
}

#[derive(Debug, Clone, Parser)]
struct AirdropArgs {
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_SQLITE_PATH",
        default_value = "artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite"
    )]
    sqlite_path: String,
    #[arg(long)]
    output: String,
    #[arg(long, value_enum, default_value_t = AirdropFormatArg::Json)]
    format: AirdropFormatArg,
}

#[derive(Debug, Clone, Parser)]
struct RebuildNormalizedArgs {
    #[arg(long, value_enum, default_value_t = StorageBackend::Sqlite)]
    backend: StorageBackend,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_SQLITE_PATH",
        default_value = "artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite"
    )]
    sqlite_path: String,
    #[arg(long, env = "PAPERPROOF_INDEXER_POSTGRES_URL")]
    postgres_url: Option<String>,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    clear_existing: bool,
}

#[derive(Debug, Clone, Parser)]
struct DeploymentCheckArgs {
    #[arg(long, default_value_t = false)]
    hard_fail: bool,
}

#[derive(Debug, Clone, Parser)]
struct IndexerArgs {
    #[arg(long, env = "PAPERPROOF_INDEXER_SINK", default_value = "jsonl")]
    sink: String,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_OUT",
        default_value = "artifacts/indexer"
    )]
    output_dir: String,
    #[arg(long, env = "PAPERPROOF_INDEXER_PAGE_LIMIT", default_value_t = 50)]
    page_limit: u64,
    #[arg(long, env = "PAPERPROOF_INDEXER_PAGES", default_value_t = 1)]
    pages: u64,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_TRUST_POLICY",
        value_enum,
        default_value_t = TrustPolicyArg::Canonical
    )]
    trust_policy: TrustPolicyArg,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_FAIL_ON_REJECTED",
        action = ArgAction::Set,
        default_value_t = true
    )]
    fail_on_rejected: bool,
}

#[derive(Debug, Clone, Parser)]
struct CheckpointArgs {
    #[arg(long, env = "PAPERPROOF_INDEXER_SINK", default_value = "sqlite")]
    sink: String,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_OUT",
        default_value = "artifacts/indexer"
    )]
    output_dir: String,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_GRPC_URL",
        default_value = "https://fullnode.mainnet.sui.io:443"
    )]
    grpc_url: String,
    #[arg(long, env = "PAPERPROOF_INDEXER_START_CHECKPOINT")]
    start_checkpoint: Option<u64>,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_CHECKPOINT_COUNT",
        default_value_t = 100
    )]
    checkpoint_count: u64,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_CHECKPOINT_BATCH_SIZE",
        default_value_t = 10
    )]
    batch_size: u64,
    #[arg(long, env = "PAPERPROOF_INDEXER_WORKERS", default_value_t = 4)]
    worker_count: usize,
    #[arg(long, env = "PAPERPROOF_INDEXER_MAX_CHECKPOINTS_PER_SECOND")]
    max_checkpoints_per_second: Option<u64>,
    #[arg(long, env = "PAPERPROOF_INDEXER_RETRY_ATTEMPTS", default_value_t = 3)]
    retry_attempts: u32,
    #[arg(long, env = "PAPERPROOF_INDEXER_RETRY_DELAY_MS", default_value_t = 500)]
    retry_delay_ms: u64,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_TRUST_POLICY",
        value_enum,
        default_value_t = TrustPolicyArg::Canonical
    )]
    trust_policy: TrustPolicyArg,
}

#[derive(Debug, Clone, Parser)]
struct TailArgs {
    #[command(flatten)]
    indexer: IndexerArgs,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_TAIL_INTERVAL_MS",
        default_value_t = 10_000
    )]
    interval_ms: u64,
    #[arg(long, env = "PAPERPROOF_INDEXER_TAIL_ONCE", default_value_t = false)]
    once: bool,
}

#[derive(Debug, Clone, Parser)]
struct EnrichArgs {
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_SQLITE_PATH",
        default_value = "artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite"
    )]
    sqlite_path: String,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_WALRUS_AGGREGATOR",
        default_value = "https://aggregator.walrus-testnet.walrus.space"
    )]
    walrus_aggregator_url: String,
    #[arg(long, default_value_t = 25)]
    limit: usize,
    #[arg(long, default_value_t = 4096)]
    max_preview_bytes: usize,
}

#[derive(Debug, Clone, Parser)]
struct ServeArgs {
    #[arg(long, value_enum, default_value_t = StorageBackend::Sqlite)]
    backend: StorageBackend,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_BIND",
        default_value = "127.0.0.1:8787"
    )]
    bind: String,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_SQLITE_PATH",
        default_value = "artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite"
    )]
    sqlite_path: String,
    #[arg(long, env = "PAPERPROOF_INDEXER_POSTGRES_URL")]
    postgres_url: Option<String>,
    #[arg(
        long,
        env = "PAPERPROOF_OFFICIAL_MANIFEST_BASE_URL",
        default_value = "https://paperproof.site"
    )]
    official_manifest_base_url: String,
    #[arg(
        long,
        env = "PAPERPROOF_INDEXER_WALRUS_AGGREGATOR",
        default_value = "https://aggregator.walrus-mainnet.walrus.space"
    )]
    walrus_aggregator_url: String,
    #[arg(long, env = "PAPERPROOF_SERVE_TAIL", default_value_t = true)]
    tail: bool,
    #[arg(
        long,
        env = "PAPERPROOF_SERVE_TAIL_INTERVAL_MS",
        default_value_t = 10_000
    )]
    tail_interval_ms: u64,
    #[arg(
        long,
        env = "PAPERPROOF_OFFICIAL_REFRESH_INTERVAL_MS",
        default_value_t = 300_000
    )]
    official_refresh_interval_ms: u64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StorageBackend {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TrustPolicyArg {
    Raw,
    Canonical,
    Verified,
    VerifiedWithWalrus,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AirdropFormatArg {
    Json,
    Csv,
}

impl From<TrustPolicyArg> for IndexerTrustPolicy {
    fn from(value: TrustPolicyArg) -> Self {
        match value {
            TrustPolicyArg::Raw => Self::Raw,
            TrustPolicyArg::Canonical => Self::Canonical,
            TrustPolicyArg::Verified => Self::Verified,
            TrustPolicyArg::VerifiedWithWalrus => Self::VerifiedWithWalrus,
        }
    }
}

#[tokio::main]
async fn main() -> paperproof_sdk_rs::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Commands::Backfill(args) => {
            let config = ReferenceIndexerConfig {
                sink: args.sink,
                output_dir: args.output_dir,
                page_limit: args.page_limit,
                pages: args.pages,
                trust_policy: args.trust_policy.into(),
                fail_on_rejected: args.fail_on_rejected,
                ..Default::default()
            };
            let report = backfill(config).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::CheckpointBackfill(args) => {
            let report = checkpoint_backfill(args).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Tail(args) => {
            let config = ReferenceIndexerConfig {
                sink: args.indexer.sink,
                output_dir: args.indexer.output_dir,
                page_limit: args.indexer.page_limit,
                pages: args.indexer.pages,
                trust_policy: args.indexer.trust_policy.into(),
                fail_on_rejected: args.indexer.fail_on_rejected,
                tail_interval_ms: args.interval_ms,
                ..Default::default()
            };
            loop {
                let report = tail_once(&config).await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if args.once {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(config.tail_interval_ms)).await;
            }
        }
        Commands::Replay(args) => {
            let report = replay_jsonl_to_state(&args.input, args.output.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::RebuildNormalized(args) => {
            let report = match args.backend {
                StorageBackend::Sqlite => {
                    let report =
                        rebuild_normalized_from_sqlite_raw(&args.sqlite_path, args.clear_existing)?;
                    #[cfg(feature = "sqlite")]
                    paperproof_indexer_reference::metrics::sqlite_record_replay_metrics(
                        &args.sqlite_path,
                        &paperproof_indexer_reference::ReplayReport {
                            input_path: args.sqlite_path.clone(),
                            events_seen: report.events_seen,
                            domain_events_applied: report.events_applied,
                            output_path: None,
                        },
                    )?;
                    report
                }
                StorageBackend::Postgres => {
                    let url = args.postgres_url.ok_or_else(|| {
                        paperproof_sdk_rs::PaperProofError::invalid_input(
                            "PAPERPROOF_INDEXER_POSTGRES_URL",
                            "set --postgres-url or PAPERPROOF_INDEXER_POSTGRES_URL",
                        )
                    })?;
                    rebuild_normalized_from_postgres_raw(&url, args.clear_existing).await?
                }
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Airdrop(args) => {
            let format = match args.format {
                AirdropFormatArg::Json => paperproof_indexer_reference::AirdropFormat::Json,
                AirdropFormatArg::Csv => paperproof_indexer_reference::AirdropFormat::Csv,
            };
            let rows = export_airdrop_snapshot(&args.sqlite_path, &args.output, format)?;
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        Commands::CheckDeployment(args) => {
            let check = paperproof_sdk_rs::check_deployment_update_from_url(
                Some(paperproof_sdk_rs::MAINNET_DEPLOYMENT.clone()),
                Some(paperproof_sdk_rs::DEFAULT_MAINNET_DEPLOYMENT_MANIFEST_URL),
            )
            .await;
            println!(
                "{}",
                paperproof_sdk_rs::format_deployment_update_check(&check)
            );
            if args.hard_fail {
                paperproof_sdk_rs::enforce_deployment_update_policy(
                    &check,
                    paperproof_sdk_rs::DeploymentDriftPolicy::HardFailOnAnyProblem,
                )?;
            }
        }
        Commands::EnrichContent(args) => {
            let service = default_walrus_content_service(args.walrus_aggregator_url);
            #[cfg(feature = "sqlite")]
            let store = paperproof_indexer_reference::SqliteContentRefStore::new(args.sqlite_path)?;
            #[cfg(not(feature = "sqlite"))]
            let store = paperproof_indexer_reference::InMemoryContentRefStore::default();
            let enricher = PaperProofContentEnricher::new(service, store);
            let outputs = enricher
                .enrich_pending(args.limit, args.max_preview_bytes)
                .await?;
            println!("{}", serde_json::to_string_pretty(&outputs)?);
        }
        Commands::Serve(args) => {
            let (sqlite_path, postgres_url) = match args.backend {
                StorageBackend::Sqlite => (Some(args.sqlite_path), None),
                StorageBackend::Postgres => {
                    let url = args.postgres_url.ok_or_else(|| {
                        paperproof_sdk_rs::PaperProofError::invalid_input(
                            "PAPERPROOF_INDEXER_POSTGRES_URL",
                            "set --postgres-url or PAPERPROOF_INDEXER_POSTGRES_URL",
                        )
                    })?;
                    (None, Some(url))
                }
            };
            let api_state = ApiState {
                sqlite_path: sqlite_path.clone(),
                postgres_url,
                official_content: OfficialContentConfig {
                    manifest_base_url: args.official_manifest_base_url,
                    walrus_aggregator_url: args.walrus_aggregator_url,
                },
                ..Default::default()
            };
            spawn_official_refresh_loop(api_state.clone(), args.official_refresh_interval_ms);
            if args.tail {
                spawn_serve_tail_loop(
                    sqlite_path.clone(),
                    args.tail_interval_ms,
                    api_state.clone(),
                );
            }
            run_api_server(ApiConfig { bind: args.bind }, api_state).await?;
        }
        Commands::Schema { kind } => match kind {
            SchemaKind::Sqlite => println!("{SQLITE_REFERENCE_SCHEMA}"),
            SchemaKind::Postgres => println!("{POSTGRES_REFERENCE_SCHEMA}"),
        },
    }
    Ok(())
}

async fn backfill(
    config: ReferenceIndexerConfig,
) -> paperproof_sdk_rs::Result<paperproof_indexer_reference::BackfillReport> {
    let query = PaperProofQueryClient::mainnet();
    let indexer = PaperProofIndexerClient::new(query);
    let sink = build_event_sink(&config.sink, &config.output_dir, "backfill").await?;
    let cursor_store = build_cursor_store(&config.sink, &config.output_dir).await?;
    let report = run_backfill_once(
        &indexer,
        sink.as_ref(),
        cursor_store.as_ref(),
        config.page_limit,
        config.pages,
        config.trust_policy.clone(),
        config.fail_on_rejected,
    )
    .await?;
    record_metrics_if_sqlite(&config, &report)?;
    Ok(report)
}

#[cfg(feature = "sui-native")]
async fn checkpoint_backfill(
    args: CheckpointArgs,
) -> paperproof_sdk_rs::Result<paperproof_sdk_rs::CheckpointIngestionReport> {
    let query = PaperProofQueryClient::mainnet();
    let indexer = PaperProofIndexerClient::new(query);
    let provider = Arc::new(paperproof_sdk_rs::SuiNativeProvider::new(args.grpc_url)?);
    let sink: Arc<dyn paperproof_sdk_rs::PaperProofEventSink> =
        Arc::from(build_event_sink(&args.sink, &args.output_dir, "checkpoint").await?);
    let cursor_store: Arc<dyn paperproof_sdk_rs::IndexerCursorStore> =
        Arc::from(build_cursor_store(&args.sink, &args.output_dir).await?);
    let report = indexer
        .ingest_checkpoint_range_once(
            provider,
            sink,
            cursor_store,
            paperproof_sdk_rs::CheckpointIngestionOptions {
                start_checkpoint: args.start_checkpoint,
                checkpoint_count: args.checkpoint_count,
                batch_size: args.batch_size,
                worker_count: args.worker_count,
                max_checkpoints_per_second: args.max_checkpoints_per_second,
                retry: paperproof_sdk_rs::RetryOptions {
                    attempts: args.retry_attempts as usize,
                    base_delay_ms: args.retry_delay_ms,
                    max_delay_ms: args
                        .retry_delay_ms
                        .saturating_mul(8)
                        .max(args.retry_delay_ms),
                    ..Default::default()
                },
                canonical_only: true,
                trust_policy: args.trust_policy.into(),
            },
        )
        .await?;
    record_checkpoint_metrics(&args.sink, &args.output_dir, &report.metrics).await?;
    Ok(report)
}

#[cfg(not(feature = "sui-native"))]
async fn checkpoint_backfill(
    _args: CheckpointArgs,
) -> paperproof_sdk_rs::Result<paperproof_sdk_rs::CheckpointIngestionReport> {
    Err(paperproof_sdk_rs::PaperProofError::invalid_input(
        "sui-native",
        "checkpoint-backfill requires --features sui-native",
    ))
}

async fn tail_once(
    config: &ReferenceIndexerConfig,
) -> paperproof_sdk_rs::Result<paperproof_indexer_reference::TailReport> {
    let query = PaperProofQueryClient::mainnet();
    let indexer = PaperProofIndexerClient::new(query);
    let sink = build_event_sink(&config.sink, &config.output_dir, "tail").await?;
    let cursor_store = build_cursor_store(&config.sink, &config.output_dir).await?;
    let report = run_tail_once(
        &indexer,
        sink.as_ref(),
        cursor_store.as_ref(),
        config.page_limit,
        config.trust_policy.clone(),
        config.fail_on_rejected,
    )
    .await?;
    record_metrics_if_sqlite(config, &report)?;
    Ok(report)
}

#[cfg(feature = "sui-native")]
async fn record_checkpoint_metrics(
    sink: &str,
    output_dir: &str,
    metrics: &paperproof_sdk_rs::IndexerMetrics,
) -> paperproof_sdk_rs::Result<()> {
    let _ = output_dir;
    let _ = metrics;
    match sink {
        "sqlite" => {
            #[cfg(feature = "sqlite")]
            {
                let sqlite_path =
                    std::env::var("PAPERPROOF_INDEXER_SQLITE_PATH").unwrap_or_else(|_| {
                        format!("{output_dir}/paperproof-indexer-reference.sqlite")
                    });
                paperproof_indexer_reference::metrics::sqlite_record_checkpoint_metrics(
                    &sqlite_path,
                    metrics,
                )?;
            }
        }
        "postgres" => {
            #[cfg(feature = "postgres")]
            {
                let url = std::env::var("PAPERPROOF_INDEXER_POSTGRES_URL").map_err(|_| {
                    paperproof_sdk_rs::PaperProofError::invalid_input(
                        "PAPERPROOF_INDEXER_POSTGRES_URL",
                        "set PAPERPROOF_INDEXER_POSTGRES_URL when sink=postgres",
                    )
                })?;
                paperproof_indexer_reference::metrics::postgres_record_checkpoint_metrics(
                    &url, metrics,
                )
                .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
fn record_metrics_if_sqlite(
    config: &ReferenceIndexerConfig,
    report: &paperproof_indexer_reference::BackfillReport,
) -> paperproof_sdk_rs::Result<()> {
    if config.sink == "sqlite" {
        let sqlite_path = std::env::var("PAPERPROOF_INDEXER_SQLITE_PATH").unwrap_or_else(|_| {
            format!("{}/paperproof-indexer-reference.sqlite", config.output_dir)
        });
        paperproof_indexer_reference::metrics::sqlite_record_ingest_metrics(&sqlite_path, report)?;
    }
    Ok(())
}

#[cfg(not(feature = "sqlite"))]
fn record_metrics_if_sqlite(
    _config: &ReferenceIndexerConfig,
    _report: &paperproof_indexer_reference::BackfillReport,
) -> paperproof_sdk_rs::Result<()> {
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "paperproof_indexer_reference=info,paperproof_sdk_rs=info".to_string()
        }))
        .try_init();
}

fn spawn_serve_tail_loop(sqlite_path: Option<String>, interval_ms: u64, api_state: ApiState) {
    let Some(sqlite_path) = sqlite_path else {
        tracing::warn!("serve tail loop is only enabled for sqlite backend");
        return;
    };
    tokio::spawn(async move {
        let output_dir = std::path::Path::new(&sqlite_path)
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "artifacts/indexer-mainnet".to_string());
        let config = ReferenceIndexerConfig {
            sink: "sqlite".to_string(),
            output_dir,
            page_limit: 50,
            trust_policy: IndexerTrustPolicy::Canonical,
            fail_on_rejected: true,
            tail_interval_ms: interval_ms,
            ..Default::default()
        };
        loop {
            match tail_once(&config).await {
                Ok(report) => {
                    if report.accepted_written > 0 {
                        tracing::info!(
                            accepted = report.accepted_written,
                            duplicates = report.duplicate_skipped,
                            "serve tail loop indexed events"
                        );
                        refresh_official_cache_after_tail(api_state.clone()).await;
                    }
                }
                Err(error) => tracing::warn!(error = %error, "serve tail loop failed"),
            }
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    });
}

fn spawn_official_refresh_loop(api_state: ApiState, interval_ms: u64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            refresh_official_cache_after_tail(api_state.clone()).await;
        }
    });
}

async fn refresh_official_cache_after_tail(api_state: ApiState) {
    match paperproof_indexer_reference::api::refresh_official_content_cache(api_state).await {
        Ok(report) => tracing::info!(
            attempted = report.attempted,
            cached = report.cached,
            failed = report.failed,
            "official content cache refreshed"
        ),
        Err(error) => tracing::warn!(error = %error, "official content cache refresh failed"),
    }
}
