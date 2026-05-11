# Airdrop Snapshot Pipeline

The reference indexer includes a deterministic participation snapshot pipeline.
It is intended as a starting point for PaperProof ecosystem analytics and
future reward design, not as a final token distribution policy.

## Source Tables

The reference snapshot reads:

- `domain_airdrop_scores`;
- indirectly, normalized tables that feed the score table.

Score updates currently happen when the reducer observes accepted events such
as:

- artifact published;
- version added;
- comment added;
- paper liked;
- proposal voted.

## Export

JSON:

```bash
cargo run --features sqlite -- airdrop \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --output artifacts/airdrop.json \
  --format json
```

CSV:

```bash
cargo run --features sqlite -- airdrop \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --output artifacts/airdrop.csv \
  --format csv
```

REST:

```bash
curl http://127.0.0.1:8787/v1/airdrop/snapshot
```

## Output Fields

- `address`;
- `published_artifacts`;
- `versions_added`;
- `comments`;
- `votes`;
- `likes`;
- `score`;
- `reasons`.

## Reference Scoring

The built-in scoring is intentionally simple and transparent. It helps test the
data pipeline and makes contribution data easy to inspect.

It should not be treated as final token economics.

## Production Snapshot Checklist

Before using a snapshot for real rewards:

1. define snapshot id;
2. define rule version;
3. publish rule description;
4. hash and archive rule config;
5. define start/end checkpoints or timestamps;
6. define included deployments;
7. define excluded addresses and reasons;
8. define anti-sybil review process;
9. rebuild normalized state from raw events;
10. export JSON and CSV;
11. sign or hash output files;
12. publish reproducibility instructions.

## Reproducibility

A robust snapshot should be reproducible from:

- PaperProof deployment manifest;
- raw accepted events;
- reducer version;
- scoring rule config;
- snapshot checkpoint range;
- export code version.

The current reference repository provides the raw/normalized/rebuild/export
foundation. Forks can add formal rule manifests and signed output.

## Risk Notes

Do not use unverified or incomplete event streams for real rewards.

Do not silently convert provider failure into empty contribution state.

Do not make final reward decisions solely from the reference score without
reviewing:

- spam;
- self-interactions;
- duplicate content;
- bot patterns;
- wash activity;
- governance manipulation;
- content availability.
