# Module Guide

This guide maps repository modules to responsibilities.

## `src/main.rs`

CLI entry point.

Responsibilities:

- parse command-line args;
- initialize tracing;
- wire query/indexer/sink/cursor components;
- run commands;
- print JSON reports.

Important commands:

- `backfill`;
- `checkpoint-backfill`;
- `tail`;
- `replay`;
- `rebuild-normalized`;
- `airdrop`;
- `check-deployment`;
- `enrich-content`;
- `serve`;
- `schema`.

## `src/pipeline.rs`

Event-query pipeline.

Responsibilities:

- scan PaperProof canonical module filters;
- load and save cursors;
- call `PaperProofIndexerClient::scan_once`;
- enforce rejected-batch policy;
- write event batches to sinks;
- produce backfill/tail reports;
- replay JSONL into SDK state.

This module is intentionally small because protocol parsing and trust checks
live in `paperproof-sdk-rs`.

## `src/store.rs`

Storage and sink composition.

Responsibilities:

- build JSONL, SQLite, and Postgres sinks;
- build cursor stores;
- wrap SDK sinks with normalized projection;
- provide file cursor store for JSONL;
- provide content ref stores.

Key type:

`ReferenceEventSink`

It writes raw accepted/rejected events through the SDK sink and then applies
local normalized projection for SQLite or Postgres.

## `src/normalized.rs`

Domain reducer and query layer.

Responsibilities:

- apply accepted events into normalized tables;
- implement SQLite projection;
- implement Postgres projection;
- rebuild normalized tables from raw events;
- expose SQLite query methods;
- expose Postgres query methods;
- export airdrop rows;
- convert SQL rows into API records.

Important record types:

- `ArtifactRecord`;
- `VersionRecord`;
- `CommentRecord`;
- `GovernanceProposalRecord`;
- `GovernanceVoteRecord`;
- `ActivityRecord`;
- `AirdropRow`;
- `RebuildReport`.

## `src/api.rs`

REST API server.

Responsibilities:

- expose read-only API endpoints;
- support SQLite and Postgres backends;
- expose JSON metrics;
- expose Prometheus metrics;
- return structured JSON errors.

The API reads normalized tables. It does not scan Sui and does not fetch Walrus
content directly.

## `src/content.rs`

Walrus content enrichment.

Responsibilities:

- abstract Walrus content source;
- read pending content refs;
- fetch blob bytes;
- calculate SHA-256;
- compare expected digest;
- extract UTF-8 preview;
- return enrichment output.

Important types:

- `WalrusContentSource`;
- `PaperProofContentEnricher`;
- `ContentEnrichmentInput`;
- `ContentEnrichmentOutput`;
- `ContentEnrichmentStatus`.

## `src/metrics.rs`

Metrics persistence and formatting.

Responsibilities:

- record ingestion metrics;
- record checkpoint worker metrics;
- read SQLite metrics;
- read Postgres metrics;
- render Prometheus text format.

Important type:

`IndexerMetricSnapshot`

## `src/config.rs`

Shared configuration defaults.

Responsibilities:

- network name enum;
- default indexer settings;
- tail interval;
- Walrus aggregator default.

## `src/schema.rs`

Embeds migrations:

- `migrations/sqlite/001_reference.sql`;
- `migrations/postgres/001_reference.sql`.

## Test Modules

`tests/api.rs`

Local HTTP server tests over SQLite.

`tests/normalized_flow.rs`

End-to-end normalized projection and rebuild tests over SQLite.

`tests/content_enrichment.rs`

Mock Walrus source tests.

`tests/normalized.rs`

Schema presence tests.

`tests/config.rs`

Trust policy default tests.

## Dependency Boundary

Protocol-aware logic should stay in `paperproof-sdk-rs`:

- deployment definitions;
- event parsing;
- trust checks;
- provider abstraction;
- checkpoint worker;
- Sui gRPC adapter;
- raw sink table definitions.

Reference-indexer-specific logic should stay here:

- normalized product tables;
- API views;
- airdrop reference scoring;
- Walrus enrichment orchestration;
- docs and deployment examples.
