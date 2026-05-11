# Replay and Rebuild

Replay is one of the most important indexer properties.

The core idea:

```text
chain facts -> raw events -> normalized tables -> API views / analytics
```

If reducer logic changes, you should be able to rebuild normalized state from
raw events without rescanning Sui.

## Why Replay Matters

Replay lets operators safely change:

- artifact display rules;
- version selection logic;
- comment-tree rendering;
- governance history views;
- content-ref extraction;
- search documents;
- moderation flags;
- ranking and feed generation;
- airdrop scoring;
- analytics summaries.

Without replay, every reducer change requires a slow chain rescan.

## Raw Event Replay

Raw event tables preserve:

- event key;
- checkpoint;
- transaction digest;
- event sequence;
- package/module/event type;
- sender;
- timestamp;
- parsed JSON.

This is enough to re-run deterministic reducers.

## JSONL Replay

`replay` reads accepted-event JSONL and builds SDK domain state:

```bash
cargo run -- replay \
  --input artifacts/indexer/backfill-accepted.jsonl \
  --output artifacts/indexer/state.json
```

Use this for:

- debugging parser behavior;
- quick state summaries;
- checking raw event exports.

It does not write SQL normalized tables.

## SQLite Rebuild

```bash
cargo run --features sqlite -- rebuild-normalized \
  --backend sqlite \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite
```

This:

1. keeps raw event tables;
2. clears normalized tables;
3. reads accepted raw events ordered by checkpoint/digest/event sequence;
4. applies reducers;
5. recreates normalized domain state.

## Postgres Rebuild

```bash
export PAPERPROOF_INDEXER_POSTGRES_URL="postgres://user:pass@localhost/paperproof"
cargo run --features postgres -- rebuild-normalized \
  --backend postgres
```

Use this for production-like deployments where raw facts and API state live in
Postgres.

## What Gets Cleared

Rebuild clears derived state:

- `paperproof_content_refs`;
- `paperproof_content_cache`;
- `domain_activity`;
- `domain_votes`;
- `domain_governance_proposals`;
- `domain_comments`;
- `domain_versions`;
- `domain_artifacts`;
- `domain_airdrop_scores`.

It does not clear:

- accepted raw events;
- rejected raw events;
- cursors;
- processed event ids;
- metrics tables.

## Operational Checklist

Before production rebuild:

1. stop ingestion workers;
2. back up the database;
3. confirm deployment manifest;
4. confirm reducer version;
5. run rebuild;
6. compare summary counts;
7. run API smoke tests;
8. restart tail/checkpoint workers;
9. monitor rejected events and checkpoint lag.

## Determinism

Reducers should be deterministic:

- same raw events;
- same deployment manifest;
- same reducer code;
- same rule config;
- same output.

If you add time-dependent ranking, moderation, or search logic, keep it outside
the core replayable reducer or store the rule inputs explicitly.
