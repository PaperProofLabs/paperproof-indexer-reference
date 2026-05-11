# Architecture

PaperProof Indexer Reference is an official reference implementation, not the
only valid way to index PaperProof Protocol. Its main purpose is to show how to
turn on-chain PaperProof facts into replayable raw records, normalized domain
tables, API responses, analytics summaries, and optional Walrus content cache.

The design goal is conservative and modular:

- keep Sui chain facts replayable;
- keep PaperProof protocol parsing inside `paperproof-sdk-rs`;
- keep application-specific views replaceable;
- never treat provider failure as empty state;
- make forks easy for community sites, explorers, analytics tools, and airdrop
  pipelines.

## High-Level Flow

```text
Sui GraphQL / Sui gRPC checkpoints
        |
        v
paperproof-sdk-rs
  - deployment filters
  - canonical / verified trust checks
  - event parsing
  - checkpoint worker
        |
        v
raw event sink
  - JSONL
  - SQLite
  - Postgres
        |
        v
normalized projection
  - artifacts
  - versions
  - comments
  - governance
  - activity
  - airdrop scores
        |
        v
REST API / metrics / exports

Walrus content refs
        |
        v
enrichment worker
  - read blob
  - verify hash
  - store preview/cache
```

The Sui fact pipeline and the Walrus enrichment pipeline are deliberately
separate. A slow or unavailable Walrus aggregator should not block indexing of
protocol facts.

## Main Modules

`src/main.rs`

CLI entry point. It wires commands such as `backfill`, `tail`,
`checkpoint-backfill`, `rebuild-normalized`, `serve`, `airdrop`,
`enrich-content`, and `check-deployment`.

`src/pipeline.rs`

Event-query based backfill/tail orchestration. It scans canonical PaperProof
module filters, writes batches to a sink, saves cursors, and returns compact
reports.

`src/store.rs`

Storage adapters and wrappers. It builds JSONL, SQLite, and Postgres sinks and
cursor stores. `ReferenceEventSink` wraps the SDK raw sink and applies local
normalized projections.

`src/normalized.rs`

Domain projection and query layer. It materializes accepted PaperProof events
into normalized tables, supports SQLite and Postgres projection, supports
SQLite/Postgres API queries, and implements raw-to-normalized rebuild.

`src/api.rs`

REST API server. It serves Explorer, Artifact Detail, Governance, My Space,
Activity, Airdrop, Analytics, and Prometheus metrics over SQLite or Postgres
backends.

`src/content.rs`

Walrus enrichment pipeline. It reads pending content refs, fetches blobs through
a Walrus aggregator, verifies SHA-256 when available, and stores preview/cache
metadata.

`src/metrics.rs`

Metrics persistence and Prometheus exposition. It records ingestion counters,
checkpoint worker metrics, duplicate skips, DB write latency, retries, and
checkpoint lag.

`src/schema.rs`

Embeds the SQLite and Postgres reference migration files.

`src/config.rs`

Shared configuration defaults.

## Data Layers

### Raw Facts

Raw facts are the durable audit log:

- accepted PaperProof events;
- rejected events and reasons;
- cursors;
- processed event ids.

Raw facts are provided mostly by `paperproof-sdk-rs` sink tables. They should be
preserved even when normalized tables are rebuilt.

### Normalized Domain Tables

Normalized tables are deterministic, replayable projections:

- `domain_artifacts`;
- `domain_versions`;
- `domain_comments`;
- `domain_governance_proposals`;
- `domain_votes`;
- `domain_activity`;
- `domain_airdrop_scores`;
- `paperproof_content_refs`;
- `paperproof_content_cache`;
- `paperproof_indexer_metrics`;
- `paperproof_indexer_metric_samples`.

They are suitable for app queries and exports, but they are not the source of
truth. The chain and raw event layer remain the source of truth.

### API Views

The REST API reads normalized tables and returns simple JSON records. It is
intended for:

- the official PaperProof web app;
- community explorers;
- type-specific websites;
- analytics dashboards;
- airdrop snapshot tooling;
- scripts and notebooks.

## Trust Model

The indexer inherits the hardened trust model from `paperproof-sdk-rs`.

`canonical`

Default for broad indexing. It checks configured package/module/deployment
identity and avoids accepting arbitrary events as PaperProof facts.

`verified`

Stronger mode for events where SDK object-binding verification is available.
Use this for governance history, rewards, airdrops, and business-critical
statistics.

`verified-with-walrus`

Reserved for workflows that also require referenced Walrus content to be
readable and hash-verified.

`raw`

Debugging mode only. Do not use it for public Explorer state or reward logic.

## Event Query vs Checkpoint Ingestion

The repository supports two ingestion styles.

Event query backfill/tail:

- simpler;
- useful for quick start;
- suitable for small deployments and testing;
- uses canonical module filters from the SDK.

Checkpoint ingestion:

- uses the Sui Rust gRPC SDK adapter through `paperproof-sdk-rs`;
- supports workers, retries, rate limiting, and checkpoint resume;
- better aligned with Sui's long-term direction;
- currently supports canonical checkpoint indexing, not silent verified
  downgrade.

Use event-query mode for quick experiments. Use checkpoint mode for production
or high-throughput indexing.

## Replaceable Boundaries

Third-party indexers can replace:

- event source: GraphQL, gRPC checkpoint, archival service;
- raw sink: SQLite, Postgres, S3, Kafka, BigQuery;
- normalized reducer: custom business state;
- search backend: Postgres FTS, Meilisearch, Elastic;
- content enrichment: PDF text extraction, Markdown rendering, dataset parser;
- API layer: REST, GraphQL, gRPC;
- analytics and airdrop rules.

Keep canonical validation and deployment drift checks if your fork claims to
track official PaperProof deployments.
