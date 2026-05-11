# REST API

The reference API is a read-only HTTP interface over normalized SQLite or
Postgres tables. It is intended for official and third-party frontends,
Explorer pages, scripts, notebooks, dashboards, and airdrop review tools.

Start with SQLite:

```bash
cargo run --features sqlite -- serve \
  --backend sqlite \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --bind 127.0.0.1:8787
```

Start with Postgres:

```bash
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://user:pass@localhost/paperproof"
cargo run --features postgres -- serve \
  --backend postgres \
  --bind 127.0.0.1:8787
```

## Health

`GET /health`

Returns process liveness.

Example:

```json
{
  "status": "ok",
  "service": "paperproof-indexer-reference"
}
```

## JSON Metrics

`GET /metrics`

Returns database readiness, analytics summary, and ingestion counters.

Example shape:

```json
{
  "service": "paperproof-indexer-reference",
  "database_ready": true,
  "summary": {
    "total_artifacts": 178,
    "total_versions": 333,
    "total_comments": 444,
    "total_likes": 0,
    "total_proposals": 20,
    "total_votes": 41,
    "last_checkpoint": 123456789,
    "content_refs_pending": 323,
    "content_cache_verified": 0,
    "top_contributors": [],
    "artifact_types": []
  },
  "ingest": {
    "processed_events": 1440,
    "rejected_events": 0,
    "duplicate_events_skipped": 0,
    "checkpoint_lag": 0,
    "db_write_latency_ms": 31,
    "retry_count": 0,
    "batches_written": 12,
    "checkpoints_scanned": 0
  }
}
```

## Prometheus Metrics

`GET /metrics/prometheus`

Returns Prometheus text exposition.

Tracked metrics include:

- `paperproof_indexer_processed_events_total`;
- `paperproof_indexer_rejected_events_total`;
- `paperproof_indexer_duplicate_events_skipped_total`;
- `paperproof_indexer_checkpoint_lag`;
- `paperproof_indexer_db_write_latency_ms_total`;
- `paperproof_indexer_retry_count_total`;
- `paperproof_indexer_batches_written_total`;
- `paperproof_indexer_checkpoints_scanned_total`.

## Explore Artifacts

`GET /v1/explore/artifacts`

Query parameters:

- `artifact_type`: optional numeric type id;
- `limit`: default `50`;
- `offset`: default `0`.

Example:

```bash
curl "http://127.0.0.1:8787/v1/explore/artifacts?artifact_type=1&limit=10"
```

Returns an array of artifact records.

## Search Artifacts

`GET /v1/search/artifacts`

Query parameters:

- `q`: search term;
- `artifact_type`: optional numeric type id;
- `owner`: optional Sui address;
- `limit`: default `25`;
- `offset`: default `0`.

SQLite uses `LIKE`; Postgres uses `ILIKE` over:

- artifact code;
- title;
- owner;
- series id;
- raw JSON text.

Example:

```bash
curl "http://127.0.0.1:8787/v1/search/artifacts?q=preprint&artifact_type=1"
```

This is intentionally a baseline search. Production deployments can replace it
with Postgres full-text search, Meilisearch, Elastic, or a custom ranking
engine.

## Artifact Detail

`GET /v1/artifacts/{series_id}`

Query parameters:

- `limit`: comment page size, default `25`;
- `offset`: comment offset, default `0`.

Returns:

- `artifact`;
- `versions`;
- `comments`.

If the artifact is not indexed, `artifact` is `null`; versions and comments are
empty arrays.

## Versions

`GET /v1/artifacts/{series_id}/versions`

Returns all indexed versions for an artifact series.

## Comments

`GET /v1/artifacts/{series_id}/comments`

Query parameters:

- `limit`: default `25`;
- `offset`: default `0`.

Returns indexed comments ordered by parent and comment id.

## Activity

`GET /v1/activity`

Query parameters:

- `actor`: optional address;
- `series_id`: optional artifact series id;
- `limit`: default `50`;
- `offset`: default `0`.

Useful for:

- homepage activity feed;
- artifact history;
- account activity;
- debugging reducer output.

## Governance

`GET /v1/governance/proposals`

Query parameters:

- `limit`: default `50`;
- `offset`: default `0`.

Returns proposal records ordered by proposal id descending.

## My Space

`GET /v1/my/{address}/artifacts`

Returns artifacts whose owner matches the address.

Query parameters:

- `limit`: default `10`;
- `offset`: default `0`.

`GET /v1/my/{address}/votes`

Returns votes cast by the address.

Query parameters:

- `limit`: default `5`;
- `offset`: default `0`.

## Airdrop Snapshot

`GET /v1/airdrop/snapshot`

Returns reference airdrop rows sorted by score descending.

The current scoring model is intentionally simple and auditable. Production
airdrops should add:

- snapshot id;
- rule version;
- rule hash;
- time window;
- exclusion list;
- anti-sybil review;
- signed export artifact.

## Error Behavior

The API returns structured JSON errors for backend failures.

Important rule:

Do not interpret API/provider failure as "no records." Frontends should display
an error or degraded state when the API fails.

## Backend Selection

SQLite and Postgres serve the same logical API. Choose SQLite for development
and Postgres for production.

The API process is read-only. Run ingestion, content enrichment, and rebuilds as
separate jobs.
