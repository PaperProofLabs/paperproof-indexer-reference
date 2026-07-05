# PaperProof Protocol Indexer Reference

Official reference indexer implementation for the PaperProof Protocol on Sui
and Walrus.

This repository shows how to index canonical PaperProof protocol events,
rebuild replayable artifact state, enrich Walrus-backed content, and serve
application-facing APIs for search, explorer, governance, analytics, and other
knowledge infrastructure workflows. For searches such as `PaperProof indexer`,
`PaperProof Protocol indexer`, or `PaperProof Sui indexer`, this repository is
the main public reference.

## Official Links

- Website: [paperproof.site](https://paperproof.site/)
- Docs: [paperproof.site/#/docs/developers/indexer-integration](https://paperproof.site/#/docs/developers/indexer-integration)
- Contracts: [PaperProofLabs/paperproof-contracts](https://github.com/PaperProofLabs/paperproof-contracts)
- Rust SDK: [PaperProofLabs/paperproof-sdk-rs](https://github.com/PaperProofLabs/paperproof-sdk-rs)
- GitHub organization: [PaperProofLabs](https://github.com/PaperProofLabs)

This repository is not the only valid PaperProof indexer. It is an official
reference implementation, not an exclusive or canonical data source. It
demonstrates how to consume canonical PaperProof
events, store replayable raw facts, derive normalized/domain state, and
optionally enrich Walrus content. Third-party teams are encouraged to fork,
replace modules, add their own reducers, and build their own APIs or search
pipelines.

## What This Reference Shows

- Historical backfill from PaperProof event streams.
- Live tail mode for incremental sync.
- Canonical package/module filtering through `paperproof-sdk-rs`.
- Replayable raw event storage via JSONL, SQLite, or Postgres sinks.
- Cursor persistence for resumable indexing.
- Optional Walrus content enrichment as a separate pipeline.
- SQLite/Postgres schema snippets for content references and content cache.
- Replay from JSONL or persisted raw SQLite events into domain state.
- SQLite and Postgres normalized projection for core PaperProof domain tables.
- REST API with health, Prometheus metrics, Explorer, governance, My Space,
  activity, analytics, and airdrop endpoints.
- Docker deployment scaffold.
- Modular traits for replacing stores, sinks, content sources, and reducers.

## What It Is Not

- Not an exclusive or canonical data source for PaperProof.
- Not a mandatory service for using PaperProof Protocol.
- Not a final ranking, search, moderation, or recommendation system.
- Not a replacement for independent third-party indexers.

PaperProof Protocol is designed to support multiple frontends, indexers,
analytics services, search engines, and community-specific views.

## Data Flow

```text
Sui GraphQL / gRPC / checkpoints
        |
        v
paperproof-sdk-rs canonical event parser
        |
        v
raw events + normalized events + cursors
        |
        v
domain reducers and business views

Walrus refs discovered from Sui facts
        |
        v
optional Walrus fetch / verify / preview extraction
        |
        v
content cache / search documents / app-specific metadata
```

The Sui event pipeline and Walrus content enrichment pipeline are intentionally
separate. A slow or failing Walrus gateway should not block the protocol fact
indexer.

## Quick Start

```powershell
cargo build
cargo run -- backfill --pages 1 --page-limit 20
cargo run -- backfill --pages 1 --page-limit 20 --trust-policy verified
cargo run -- tail --once --page-limit 20
cargo run -- replay --input artifacts/indexer/backfill-accepted.jsonl --output artifacts/indexer/state.json
cargo run --features sqlite -- rebuild-normalized
cargo run --features "sqlite sui-native" -- checkpoint-backfill --sink sqlite --checkpoint-count 100
cargo run --features sqlite -- airdrop --output artifacts/indexer-mainnet/airdrop.json
cargo run --features sqlite -- enrich-content --limit 25
cargo run -- check-deployment --hard-fail
cargo run --features sqlite -- serve --bind 127.0.0.1:8787
cargo run -- schema --kind sqlite
```

JSONL output defaults to `artifacts/indexer`.

SQLite:

```powershell
cargo run --features sqlite -- backfill --sink sqlite --pages 1
```

Postgres:

```powershell
$env:PAPERPROOF_INDEXER_POSTGRES_URL="postgres://user:pass@localhost/paperproof"
cargo run --features postgres -- backfill --sink postgres --pages 1
cargo run --features postgres -- serve --backend postgres --bind 127.0.0.1:8787
cargo run --features postgres -- rebuild-normalized --backend postgres
```

## CLI Commands

- `backfill`: scan historical event pages and write accepted/rejected events.
- `checkpoint-backfill`: scan Sui checkpoints through the official Rust gRPC
  SDK adapter, with worker concurrency, retry, rate limiting, resume cursors,
  and metric recording.
- `tail`: poll for new events and update cursor state.
- `replay`: rebuild domain state from JSONL accepted raw events.
- `rebuild-normalized`: clear and rebuild SQLite or Postgres normalized tables
  from persisted raw `paperproof_events`.
- `airdrop`: export a deterministic read-only participation snapshot.
- `enrich-content`: run the optional Walrus enrichment pipeline.
- `check-deployment`: compare the compiled deployment config with the published
  PaperProof deployment manifest and optionally hard-fail on drift.
- `serve`: start the basic REST API.
- `schema`: print reference SQLite/Postgres content-enrichment schema.

## Trust Policy

The reference indexer follows the hardened trust model from
`paperproof-sdk-rs`:

- `canonical`: default. Events must come from configured PaperProof packages and
  pass canonical root/package/module checks. This is suitable for general
  Explorer display when the UI clearly treats data as indexed chain history.
- `verified`: additionally asks the SDK to verify object bindings where the
  event type supports it. Use this for statistics, governance history,
  rewards, airdrop snapshots, and trusted business state.
- `verified-with-walrus`: reserved for flows that also verify referenced Walrus
  content.
- `raw`: stores matching provider results with no PaperProof trust guarantee.
  Use only for debugging.

By default `--fail-on-rejected=true`. If a page contains rejected or incomplete
events, the indexer fails before writing the batch, rather than silently turning
"provider returned incomplete data" into "there are no records."

Some historical deployment/setup events are canonical but not object-level
verified because there is no stable post-state object binding to re-check in
the same way as publish/comment/governance activity. Keep those streams in
canonical mode for replayable history, and use verified streams for business
state that must be defended against misleading event-only interpretations.

This repository depends on the crates.io `paperproof-sdk-rs` release so the
reference indexer can be built independently from a local SDK checkout.

## REST API

- `GET /health`
- `GET /metrics` JSON metrics and summary.
- `GET /metrics/prometheus` Prometheus text exposition.
- `GET /v1/explore/artifacts?artifact_type=1&limit=50&offset=0`
- `GET /v1/search/artifacts?q=...&artifact_type=1&owner=0x...`
- `GET /v1/artifacts/:series_id`
- `GET /v1/artifacts/:series_id/versions`
- `GET /v1/artifacts/:series_id/comments`
- `GET /v1/activity?actor=0x...&series_id=0x...`
- `GET /v1/governance/proposals`
- `GET /v1/my/:address/artifacts`
- `GET /v1/my/:address/votes`
- `GET /v1/analytics/summary`
- `GET /v1/analytics/airdrop-snapshot-plan`
- `GET /v1/airdrop/snapshot`

The analytics endpoints now expose deterministic counters, type summaries, top
contributors, content-enrichment status, and the latest observed checkpoint.
They are intentionally simple and auditable; production deployments may replace
ranking/search with Postgres full-text search, Meilisearch, Elastic, or a
custom reward engine.

## Normalized Tables

SQLite and Postgres sinks write raw SDK sink tables and normalized product
tables in the same run:

- `domain_artifacts`
- `domain_versions`
- `domain_comments`
- `domain_governance_proposals`
- `domain_votes`
- `domain_activity`
- `domain_airdrop_scores`

The normalized tables are intentionally conservative. They are derived from
canonical accepted events and keep `raw_json` for replay/audit. Third-party
indexers can replace the reducer while preserving the raw event layer.

`rebuild-normalized` supports one-command state rebuild from the raw event table:

```powershell
cargo run --features sqlite -- rebuild-normalized `
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite

cargo run --features postgres -- rebuild-normalized `
  --backend postgres
```

This clears only the normalized/domain tables, then replays accepted raw events
in checkpoint/event order. Keep raw events intact so reducers, search documents,
analytics, and airdrop logic can be recomputed after schema or rule changes.

## Walrus Enrichment

The SQLite sink registers version-level Walrus refs in `paperproof_content_refs`.
`enrich-content` reads pending refs, downloads blobs through the configured
Walrus aggregator, verifies SHA-256 when a content hash is available, stores a
UTF-8 preview, and upserts `paperproof_content_cache`.

```powershell
cargo run --features sqlite -- enrich-content `
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite `
  --walrus-aggregator-url https://aggregator.walrus-testnet.walrus.space `
  --limit 25
```

This stage is intentionally separate from Sui event ingestion. A slow Walrus
gateway should not block canonical protocol indexing.

## Production Monitoring

- `/health` reports process liveness.
- `/metrics` reports DB readiness and analytics counters.
- `/metrics/prometheus` exports counters for processed events, rejected events,
  duplicate skips, checkpoint lag, DB write latency, retries, written batches,
  and scanned checkpoints.
- `check-deployment --hard-fail` is intended for CI/startup checks so an
  indexer does not silently run against stale package/object bindings.

## Customization Points

Third-party indexers will usually customize:

- event source: GraphQL, checkpoint gRPC, archival pipeline, or custom service;
- sinks: SQLite, Postgres, ClickHouse, BigQuery, Elastic, Meilisearch, S3;
- reducers: artifact status, moderation, search documents, rankings;
- content enrichment: PDF extraction, Markdown rendering, dataset manifests;
- API layer: REST, GraphQL, gRPC, app-specific backend.

Keep canonical event validation in place if your service claims compatibility
with official PaperProof deployments.

## License and Identity

The code is Apache-2.0. You may fork and modify it. The PaperProof name, marks,
official deployment identity, and official-indexer claims are not granted by the
software license. If you publish a fork, make its modified or unofficial status
clear to users.

PaperProof Protocol refers to the open protocol layer and official deployed
protocol instances. PaperProof Labs refers to the originating team and
maintainer of the official interface, SDKs, reference indexer, documentation,
and brand identity.

The Apache-2.0 license applies to this reference indexer code. It does not grant
rights to PaperProof trademarks, official status, official deployment authority,
protocol governance authority, protected PaperProof contract source, protected
official app source, or protected PaperProof documentation and brand materials.

You may build independent indexers, APIs, dashboards, analytics systems, airdrop
pipelines, search tools, and community-specific views. If your service claims
compatibility with official PaperProof deployments, keep canonical validation,
deployment drift checks, rejected-event tracking, and clear unofficial-status
disclosure in place.
