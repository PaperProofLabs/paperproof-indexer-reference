# Testing

The reference indexer is tested at several levels:

- configuration tests;
- schema tests;
- normalized reducer tests;
- REST API tests;
- Walrus enrichment tests;
- feature-combination compile and clippy checks.

## Standard Checks

Run formatting:

```bash
cargo fmt --check
```

Default build:

```bash
cargo check --all-targets
```

SQLite tests:

```bash
cargo test --features sqlite --all-targets
```

Postgres feature tests:

```bash
cargo test --features postgres --all-targets
```

Full feature compile:

```bash
cargo check --features "sqlite postgres sui-native" --all-targets
```

Clippy:

```bash
cargo clippy --all-targets -- -D warnings
cargo clippy --features "sqlite postgres sui-native" --all-targets -- -D warnings
```

## Test Files

`tests/config.rs`

Checks default trust policy and rejected-batch behavior.

`tests/normalized.rs`

Checks schema availability and Postgres normalized/metrics table declarations.

`tests/normalized_flow.rs`

Builds mock PaperProof events and verifies:

- SQLite sink writes raw and normalized state;
- artifact projection;
- version projection;
- comment projection;
- vote and claim projection;
- airdrop score;
- content refs;
- content cache update;
- search;
- activity feed;
- rebuild from raw SQLite events.

`tests/api.rs`

Starts a local API server against a temporary SQLite database and verifies:

- `/health`;
- `/metrics`;
- `/metrics/prometheus`;
- `/v1/explore/artifacts`;
- `/v1/search/artifacts`;
- `/v1/artifacts/{series_id}`;
- `/v1/activity`;
- `/v1/governance/proposals`;
- `/v1/my/{address}/votes`;
- `/v1/airdrop/snapshot`.

The reqwest test client disables proxy use so localhost tests are not affected
by system proxy settings.

`tests/content_enrichment.rs`

Uses a mock Walrus content source and verifies:

- hash verification;
- digest mismatch;
- fetch failure;
- UTF-8 preview extraction.

## Real Postgres Testing

Feature tests compile Postgres code and validate schema strings, but they do not
require a running Postgres service.

For real integration testing:

```bash
cd deploy
docker compose up -d postgres
cd ..
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://paperproof:paperproof@localhost:5432/paperproof"
cargo run --features postgres -- backfill --sink postgres --pages 1
cargo run --features postgres -- rebuild-normalized --backend postgres
cargo run --features postgres -- serve --backend postgres --bind 127.0.0.1:8787
curl http://127.0.0.1:8787/metrics
```

## Mainnet Read Testing

Event-query backfill:

```bash
cargo run --features sqlite -- backfill --sink sqlite --pages 1 --page-limit 20
```

Checkpoint scan:

```bash
cargo run --features "sqlite sui-native" -- checkpoint-backfill \
  --sink sqlite \
  --checkpoint-count 10 \
  --batch-size 2 \
  --worker-count 2
```

Use small limits first. Then scale up after checking metrics and rejected event
counts.

## Walrus Testing

Mock tests cover the enrichment logic. For real aggregator testing:

```bash
cargo run --features sqlite -- enrich-content \
  --limit 10 \
  --walrus-aggregator-url https://aggregator.walrus.space
```

Check:

- `paperproof_content_refs.status`;
- `paperproof_content_cache.status`;
- digest mismatch count;
- fetch failed count.

## CI Recommendations

Run at least:

```bash
cargo fmt --check
cargo check --all-targets
cargo test --features sqlite --all-targets
cargo test --features postgres --all-targets
cargo clippy --features "sqlite postgres sui-native" --all-targets -- -D warnings
```

Optional CI jobs:

- Docker image build;
- Postgres service integration;
- mainnet read-only smoke test with low page limits;
- deployment drift check.

## Windows Note

On Windows, Cargo may occasionally print an incremental compilation warning
like `os error 5` when finalizing target directories. If the command exits with
success and tests passed, this is usually a local file-lock issue rather than a
functional test failure.
