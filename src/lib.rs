// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

//! Reference indexer building blocks for PaperProof Protocol.
//!
//! This crate is intentionally a reference implementation, not the only valid
//! PaperProof indexer. It demonstrates a modular pipeline:
//!
//! - Sui backfill/tail for canonical protocol facts.
//! - Replayable raw and normalized event storage.
//! - Optional Walrus content enrichment.
//! - Replaceable sources, stores, reducers, and sinks.

pub mod analytics;
pub mod api;
pub mod config;
pub mod content;
pub mod metrics;
pub mod normalized;
pub mod official_content;
pub mod pipeline;
pub mod schema;
pub mod site_analytics;
pub mod store;

pub use analytics::{AirdropSnapshotPlan, AnalyticsSummary};
pub use api::{ApiConfig, ApiState, run_api_server};
pub use config::{NetworkName, ReferenceIndexerConfig};
pub use content::{
    ContentEnrichmentInput, ContentEnrichmentOutput, ContentEnrichmentStatus,
    PaperProofContentEnricher, WalrusContentSource,
};
pub use metrics::IndexerMetricSnapshot;
pub use normalized::{
    AirdropFormat, AirdropRow, ArtifactRecord, CommentRecord, GovernanceProposalRecord,
    GovernanceVoteRecord, NormalizedQuery, RebuildReport, VersionObjectHydrationReport,
    VersionRecord, export_airdrop_snapshot, hydrate_version_objects_postgres,
    hydrate_version_objects_sqlite, rebuild_normalized_from_postgres_raw,
    rebuild_normalized_from_sqlite_raw,
};
pub use pipeline::{
    BackfillReport, ReplayReport, TailReport, replay_jsonl_to_state, run_backfill_once,
    run_tail_once,
};
pub use schema::{POSTGRES_REFERENCE_SCHEMA, SQLITE_REFERENCE_SCHEMA};
#[cfg(feature = "sqlite")]
pub use store::SqliteContentRefStore;
pub use store::{
    ContentRef, ContentRefStore, FileCursorStore, InMemoryContentRefStore, ReferenceEventSink,
    build_cursor_store, build_event_sink,
};
