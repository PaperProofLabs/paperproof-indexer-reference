# Metrics and Monitoring

The reference indexer exposes operational metrics through:

- JSON endpoint: `/metrics`;
- Prometheus endpoint: `/metrics/prometheus`;
- database tables: `paperproof_indexer_metrics` and
  `paperproof_indexer_metric_samples`.

## Metrics Endpoint

Start the API:

```bash
cargo run --features sqlite -- serve --backend sqlite --bind 127.0.0.1:8787
```

Read JSON:

```bash
curl http://127.0.0.1:8787/metrics
```

Read Prometheus:

```bash
curl http://127.0.0.1:8787/metrics/prometheus
```

## Counters

`paperproof_indexer_processed_events_total`

Accepted PaperProof events processed by the indexer.

`paperproof_indexer_rejected_events_total`

Events rejected by trust checks, deployment filters, or parser logic.

`paperproof_indexer_duplicate_events_skipped_total`

Duplicate event writes skipped by idempotent sinks.

`paperproof_indexer_checkpoint_lag`

Estimated lag for checkpoint ingestion. In the current reference implementation
this is populated by checkpoint worker reports.

`paperproof_indexer_db_write_latency_ms_total`

Cumulative database write latency in milliseconds.

`paperproof_indexer_retry_count_total`

Retry attempts made by checkpoint workers.

`paperproof_indexer_batches_written_total`

Batches written by ingestion jobs.

`paperproof_indexer_checkpoints_scanned_total`

Checkpoints scanned by checkpoint ingestion.

## Analytics Summary

The JSON `/metrics` endpoint also includes `summary`:

- total artifacts;
- total versions;
- total comments;
- total likes;
- total proposals;
- total votes;
- last indexed checkpoint;
- pending content refs;
- verified content cache count;
- top contributors;
- artifact type summary.

This summary is useful for dashboards and quick health checks.

## Suggested Alerts

Alert if:

- rejected events become non-zero in a verified/canonical production stream;
- checkpoint lag grows for a sustained period;
- retry count rises quickly;
- DB write latency rises sharply;
- processed events stop increasing while new checkpoints are expected;
- content fetch failures grow quickly;
- deployment drift check fails.

## Database Metrics Tables

`paperproof_indexer_metrics`

Stores latest cumulative values by metric name.

`paperproof_indexer_metric_samples`

Stores append-only samples for simple historical analysis.

Production systems can replace this with Prometheus remote write, OpenTelemetry,
or another time-series backend.

## Limitations

The current metrics are intentionally simple. They are enough for smoke checks
and lightweight production visibility, but not a full observability stack.

Recommended future additions:

- histogram buckets for DB latency;
- request latency metrics for REST API;
- per-endpoint API status counts;
- per-aggregator Walrus latency;
- queue depth for enrichment workers;
- richer checkpoint lag computed against latest known Sui checkpoint.
