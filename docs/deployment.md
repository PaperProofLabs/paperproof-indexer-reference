# Deployment

This document describes deployment patterns for
`paperproof-indexer-reference`.

The reference deployment should be understood as a set of separate processes:

- backfill worker;
- tail or checkpoint worker;
- REST API server;
- Walrus enrichment worker;
- deployment drift checker;
- database;
- monitoring stack.

Do not run every workload in one process for production.

## Local SQLite Deployment

SQLite is good for local development and small demos.

```bash
cargo run --features sqlite -- backfill --sink sqlite --pages 10
cargo run --features sqlite -- serve --backend sqlite --bind 127.0.0.1:8787
```

Default SQLite path:

```text
artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite
```

Override:

```bash
export PAPERPROOF_INDEXER_SQLITE_PATH=/data/paperproof.sqlite
```

Limitations:

- one writer at a time is safest;
- not ideal for multiple long-running workers;
- not ideal for public high-traffic APIs.

## Postgres Deployment

Postgres is recommended for production.

```bash
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://paperproof:paperproof@localhost:5432/paperproof"

cargo run --features postgres -- backfill --sink postgres --pages 100
cargo run --features postgres -- serve --backend postgres --bind 0.0.0.0:8787
```

Postgres advantages:

- durable raw facts and cursors;
- concurrent API and workers;
- backups and replication;
- future full-text search;
- better operational visibility.

## Docker

Build:

```bash
docker build -t paperproof-indexer-reference .
```

Run API:

```bash
docker run --rm -p 8787:8787 paperproof-indexer-reference
```

Health:

```bash
curl http://127.0.0.1:8787/health
```

## Docker Compose

```bash
cd deploy
docker compose up --build
```

The compose file starts Postgres and the API container. Run workers as separate
jobs:

```bash
docker compose run --rm api paperproof-indexer-reference backfill --sink postgres --pages 100
docker compose run --rm api paperproof-indexer-reference tail --sink postgres
docker compose run --rm api paperproof-indexer-reference rebuild-normalized --backend postgres
```

Checkpoint worker:

```bash
docker compose run --rm api paperproof-indexer-reference checkpoint-backfill \
  --sink postgres \
  --checkpoint-count 1000 \
  --worker-count 8
```

## Recommended Production Topology

```text
                    +-------------------+
                    |   Sui gRPC / API  |
                    +---------+---------+
                              |
               +--------------+--------------+
               |                             |
       backfill/checkpoint worker      tail worker
               |                             |
               +--------------+--------------+
                              |
                         Postgres
                              |
          +-------------------+-------------------+
          |                   |                   |
        REST API         Walrus enrich       analytics/export
          |                   |                   |
       frontend          content cache        airdrop files
```

## Process Roles

Backfill worker:

- scans history;
- writes raw events;
- writes normalized tables;
- records metrics.

Tail worker:

- polls incremental events;
- updates cursor;
- keeps API state fresh.

Checkpoint worker:

- uses Sui gRPC checkpoint reads;
- supports worker concurrency;
- supports checkpoint resume;
- records checkpoint metrics.

API server:

- serves read-only REST endpoints;
- should not do heavy writes;
- should run behind a reverse proxy.

Walrus enrichment:

- fetches content refs;
- verifies hash;
- writes preview/cache;
- should be retryable and isolated from fact indexing.

## Startup Checks

Run deployment drift check before starting public workers:

```bash
cargo run -- check-deployment --hard-fail
```

Recommended startup order:

1. database;
2. migration/schema bootstrap;
3. deployment drift check;
4. backfill or checkpoint worker;
5. tail worker;
6. REST API;
7. Walrus enrichment worker;
8. monitoring scrape.

## Monitoring

Health endpoint:

```bash
curl http://127.0.0.1:8787/health
```

JSON metrics:

```bash
curl http://127.0.0.1:8787/metrics
```

Prometheus metrics:

```bash
curl http://127.0.0.1:8787/metrics/prometheus
```

Watch:

- processed event count;
- rejected event count;
- duplicate skips;
- checkpoint lag;
- retry count;
- DB write latency;
- content fetch failures;
- database storage growth.

## Backups

Back up:

- raw event tables;
- cursor tables;
- normalized tables if rebuild time matters;
- content cache if enrichment is expensive;
- exported airdrop snapshots.

Raw events are the most important data. Normalized tables can be rebuilt from
raw events.

## Upgrade Procedure

When PaperProof contracts or SDK deployment manifest changes:

1. pull the new code;
2. run tests;
3. run `check-deployment --hard-fail`;
4. stop workers;
5. back up database;
6. run schema migrations;
7. rebuild normalized tables if reducer behavior changed;
8. restart workers;
9. compare metrics and API summaries.

## Security Notes

- Do not expose Postgres publicly.
- Do not log private keys or wallet secrets.
- This indexer is read-only for protocol indexing; transaction signing should
  stay outside it.
- Treat `raw` trust policy as debugging only.
- Treat API failure as failure, not empty state.
- Keep PaperProof deployment manifests pinned and checked.

## Scaling Notes

Scale horizontally by separating:

- ingestion workers;
- API replicas;
- content enrichment workers;
- analytics/export jobs.

Avoid multiple workers writing the same event range without durable cursor and
idempotent sink behavior. Raw event tables use event keys for de-duplication,
but duplicated work still costs provider and database resources.
