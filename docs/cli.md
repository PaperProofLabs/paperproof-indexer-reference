# CLI Reference

The binary is `paperproof-indexer-reference`.

Most commands can run against JSONL, SQLite, or Postgres sinks. Feature flags
decide which backends are compiled.

Common feature combinations:

```bash
cargo run -- backfill
cargo run --features sqlite -- backfill --sink sqlite
cargo run --features postgres -- backfill --sink postgres
cargo run --features "sqlite sui-native" -- checkpoint-backfill --sink sqlite
cargo run --features "postgres sui-native" -- checkpoint-backfill --sink postgres
```

## Environment Variables

Common:

- `PAPERPROOF_INDEXER_SINK`
- `PAPERPROOF_INDEXER_OUT`
- `PAPERPROOF_INDEXER_PAGE_LIMIT`
- `PAPERPROOF_INDEXER_PAGES`
- `PAPERPROOF_INDEXER_TRUST_POLICY`
- `PAPERPROOF_INDEXER_FAIL_ON_REJECTED`

SQLite:

- `PAPERPROOF_INDEXER_SQLITE_PATH`

Postgres:

- `PAPERPROOF_INDEXER_POSTGRES_URL`

API:

- `PAPERPROOF_INDEXER_BIND`

Tail:

- `PAPERPROOF_INDEXER_TAIL_INTERVAL_MS`
- `PAPERPROOF_INDEXER_TAIL_ONCE`

Checkpoint:

- `PAPERPROOF_INDEXER_GRPC_URL`
- `PAPERPROOF_INDEXER_START_CHECKPOINT`
- `PAPERPROOF_INDEXER_CHECKPOINT_COUNT`
- `PAPERPROOF_INDEXER_CHECKPOINT_BATCH_SIZE`
- `PAPERPROOF_INDEXER_WORKERS`
- `PAPERPROOF_INDEXER_MAX_CHECKPOINTS_PER_SECOND`
- `PAPERPROOF_INDEXER_RETRY_ATTEMPTS`
- `PAPERPROOF_INDEXER_RETRY_DELAY_MS`

Walrus enrichment:

- `PAPERPROOF_INDEXER_WALRUS_AGGREGATOR`

## `backfill`

Scans historical PaperProof event pages and writes accepted/rejected events.

Example:

```bash
cargo run --features sqlite -- backfill \
  --sink sqlite \
  --pages 10 \
  --page-limit 50 \
  --trust-policy canonical
```

Use cases:

- quick local indexing;
- small deployments;
- testing trust policy;
- initial data exploration.

Important options:

- `--sink`: `jsonl`, `sqlite`, or `postgres`;
- `--output-dir`: output directory for JSONL/SQLite;
- `--page-limit`: events per query page;
- `--pages`: maximum pages per module;
- `--trust-policy`: `raw`, `canonical`, `verified`, `verified-with-walrus`;
- `--fail-on-rejected`: default `true`.

Use `--fail-on-rejected=true` for public data services. It prevents incomplete
or suspicious pages from being silently persisted as if they were valid empty
state.

## `tail`

Polls for new events and updates cursor state.

Example:

```bash
cargo run --features postgres -- tail \
  --sink postgres \
  --interval-ms 10000
```

Use `--once` for cron-style incremental jobs:

```bash
cargo run --features sqlite -- tail --sink sqlite --once
```

## `checkpoint-backfill`

Scans Sui checkpoints using the official Rust gRPC SDK adapter through
`paperproof-sdk-rs`.

Requires:

- `--features sui-native`;
- a Sui gRPC endpoint;
- a sink backend.

Example:

```bash
cargo run --features "sqlite sui-native" -- checkpoint-backfill \
  --sink sqlite \
  --grpc-url https://fullnode.mainnet.sui.io:443 \
  --start-checkpoint 100000000 \
  --checkpoint-count 1000 \
  --batch-size 10 \
  --worker-count 4 \
  --max-checkpoints-per-second 20
```

Postgres example:

```bash
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://user:pass@localhost/paperproof"
cargo run --features "postgres sui-native" -- checkpoint-backfill \
  --sink postgres \
  --checkpoint-count 1000 \
  --worker-count 8
```

The checkpoint worker supports:

- worker concurrency;
- batch size;
- retry attempts;
- retry backoff;
- rate limiting;
- checkpoint cursor resume;
- canonical event filtering;
- metrics recording.

Checkpoint ingestion does not silently downgrade `verified` trust policy. Raw
checkpoint data does not always contain enough object-binding reads to satisfy
verified checks. Use canonical checkpoint ingestion for high-throughput history,
and use query-based verified flows where object-level verification is required.

## `replay`

Replays JSONL accepted events into SDK domain state and optionally writes a JSON
state file.

Example:

```bash
cargo run -- replay \
  --input artifacts/indexer/backfill-accepted.jsonl \
  --output artifacts/indexer/state.json
```

This is useful for:

- checking raw event files;
- debugging parser behavior;
- lightweight state summaries.

It does not rebuild SQL normalized tables. Use `rebuild-normalized` for that.

## `rebuild-normalized`

Clears normalized/domain tables and rebuilds them from persisted raw
`paperproof_events`.

SQLite:

```bash
cargo run --features sqlite -- rebuild-normalized \
  --backend sqlite \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite
```

Postgres:

```bash
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://user:pass@localhost/paperproof"
cargo run --features postgres -- rebuild-normalized \
  --backend postgres
```

Use this after changing:

- reducer logic;
- airdrop scoring;
- search fields;
- content-ref extraction;
- analytics views.

It does not delete raw event tables.

## `airdrop`

Exports reference airdrop scores.

Example:

```bash
cargo run --features sqlite -- airdrop \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --output artifacts/indexer-mainnet/airdrop.csv \
  --format csv
```

Formats:

- `json`;
- `csv`.

The current scoring model is a reference model, not a final token-distribution
policy.

## `enrich-content`

Fetches pending Walrus content refs and updates content cache.

Example:

```bash
cargo run --features sqlite -- enrich-content \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --walrus-aggregator-url https://aggregator.walrus.space \
  --limit 25 \
  --max-preview-bytes 4096
```

This command is intentionally separate from Sui indexing. Run it as a worker or
cron job.

## `serve`

Starts the read-only REST API.

SQLite:

```bash
cargo run --features sqlite -- serve \
  --backend sqlite \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --bind 127.0.0.1:8787
```

Postgres:

```bash
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://user:pass@localhost/paperproof"
cargo run --features postgres -- serve \
  --backend postgres \
  --bind 127.0.0.1:8787
```

The API process should not run heavy ingestion in the same process in
production. Use separate workers.

## `check-deployment`

Checks SDK deployment configuration against the published PaperProof deployment
manifest.

Example:

```bash
cargo run -- check-deployment --hard-fail
```

Use this in CI and startup checks so indexers do not silently run with stale
package or object bindings.

## `schema`

Prints SQLite or Postgres reference schema.

```bash
cargo run -- schema --kind sqlite
cargo run -- schema --kind postgres
```

Useful for:

- migrations;
- review;
- custom database bootstrap;
- docs generation.
