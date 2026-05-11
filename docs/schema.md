# Schema

This document describes the reference schema used by
`paperproof-indexer-reference`.

The schema has two layers:

1. raw SDK sink tables, provided by `paperproof-sdk-rs`;
2. reference normalized tables, provided by this repository.

The raw layer is the durable audit log. The normalized layer is a replayable
business view for APIs, Explorer pages, airdrop snapshots, and analytics.

## Raw SDK Tables

The SDK sink creates tables for:

- accepted PaperProof events;
- rejected events;
- processed event ids;
- indexer cursors.

The important accepted-event columns are:

- `event_key`;
- `checkpoint`;
- `transaction_digest`;
- `event_seq`;
- `package_id`;
- `module`;
- `event_type`;
- `kind`;
- `sender`;
- `timestamp_ms`;
- `parsed_json`;
- `inserted_at`.

Do not delete accepted raw events in production. If reducer logic, search
rules, or airdrop scoring changes, raw events let you rebuild derived state
without rescanning Sui.

## Normalized Tables

### `domain_artifacts`

Current artifact-series level state.

Primary key:

- `series_id`

Typical consumers:

- Explore page;
- type pages;
- My Space published artifacts;
- search;
- analytics.

Important fields:

- `artifact_code`;
- `artifact_type`;
- `owner`;
- `latest_version_id`;
- `comments_tree_id`;
- `likes_book_id`;
- `title`;
- `status`;
- `published_at`;
- `updated_at`;
- `raw_json`.

### `domain_versions`

Version-level content records.

Primary key:

- `version_id`

Important fields:

- `series_id`;
- `artifact_type`;
- `version`;
- `content_hash`;
- `walrus_blob_id`;
- `content_type`;
- `created_at`;
- `raw_json`.

Every version with a Walrus blob also inserts or updates a row in
`paperproof_content_refs`.

### `domain_comments`

Comment-tree events linked to artifacts when the tree can be resolved.

Primary key:

- `(tree_id, comment_id)`

Important fields:

- `parent_comment_id`;
- `series_id`;
- `author`;
- `content_mode`;
- `status`;
- `created_at`;
- `updated_at`;
- `raw_json`.

The table supports tree rendering and paginated artifact comment views. The
actual comment content may require object reads or Walrus enrichment depending
on content mode and protocol evolution.

### `domain_governance_proposals`

Governance proposal state derived from proposal create/finalize/expire events.

Primary key:

- `proposal_id`

Important fields:

- `proposal_object_id`;
- `proposer`;
- `title`;
- `action_type`;
- `proposal_type`;
- `status`;
- `yes_votes`;
- `no_votes`;
- `created_at`;
- `updated_at`;
- `raw_json`.

### `domain_votes`

Per-voter governance participation.

Primary key:

- `(proposal_id, voter)`

Important fields:

- `side`;
- `voting_power`;
- `claimed`;
- `created_at`;
- `updated_at`;
- `raw_json`.

### `domain_activity`

Generic activity feed.

Primary key:

- `event_key`

Important fields:

- `kind`;
- `actor`;
- `series_id`;
- `proposal_id`;
- `tree_id`;
- `created_at`;
- `raw_json`.

This table is useful for homepage feeds, artifact activity, account activity,
analytics, and debugging reducer behavior.

### `domain_airdrop_scores`

Reference contribution score table.

Primary key:

- `address`

Important fields:

- `published_artifacts`;
- `versions_added`;
- `comments`;
- `votes`;
- `likes`;
- `score`;
- `reasons_json`;
- `updated_at`.

This table is intentionally a reference scoring model. Production airdrops
should define snapshot version, rule hash, time window, exclusions, and review
process.

## Walrus Tables

### `paperproof_content_refs`

Pending or processed Walrus references discovered from artifact versions.

Important fields:

- `source_event_key`;
- `artifact_id`;
- `version_id`;
- `blob_id`;
- `expected_sha256_hex`;
- `content_type`;
- `status`;
- `details_json`;
- `updated_at`.

Statuses include:

- `pending`;
- `verified`;
- `digest_mismatch`;
- `fetch_failed`.

### `paperproof_content_cache`

Content enrichment cache.

Important fields:

- `blob_id`;
- `sha256_hex`;
- `byte_len`;
- `content_type`;
- `preview_utf8`;
- `status`;
- `error`;
- `updated_at`.

Production deployments may replace this table with object storage, a document
index, or specialized extraction pipelines.

## Metrics Tables

### `paperproof_indexer_metrics`

Latest cumulative counters by metric name.

### `paperproof_indexer_metric_samples`

Append-only samples for simple time-series analysis.

Tracked names include:

- `processed_events`;
- `rejected_events`;
- `duplicate_events_skipped`;
- `checkpoint_lag`;
- `db_write_latency_ms`;
- `retry_count`;
- `batches_written`;
- `checkpoints_scanned`.

## SQLite vs Postgres

SQLite is best for:

- local development;
- demos;
- small read-only deployments;
- test fixtures;
- quick historical scans.

Postgres is best for:

- public Explorer deployments;
- concurrent backfill/tail/API processes;
- durable operations;
- analytics;
- future full-text search;
- backup and replication.

Both schemas use the same logical tables. Postgres stores JSON as `jsonb`;
SQLite stores JSON as text.

## Rebuild Rules

`rebuild-normalized` clears derived normalized tables and replays accepted raw
events into them.

It does not clear raw SDK event tables.

It is safe to rebuild when:

- reducer logic changes;
- search fields change;
- airdrop score rules change;
- content reference extraction changes;
- analytics views need to be recomputed.

Keep raw events and deployment manifests stable before running production
rebuilds.
