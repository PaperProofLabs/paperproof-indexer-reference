# PaperProof Indexer Reference Docs

This directory documents the architecture, operation, extension points, and
developer workflow for `paperproof-indexer-reference`.

Recommended reading order:

1. [Architecture](architecture.md)
2. [CLI Reference](cli.md)
3. [Schema](schema.md)
4. [REST API](api.md)
5. [Replay and Rebuild](replay.md)
6. [Walrus Enrichment](walrus-enrichment.md)
7. [Metrics and Monitoring](metrics.md)
8. [Airdrop Snapshot Pipeline](airdrop.md)
9. [Deployment](deployment.md)
10. [Testing](testing.md)
11. [Module Guide](modules.md)
12. [Third-Party Extensions](extensions.md)

## What This Indexer Is

It is an official reference implementation for indexing PaperProof Protocol.

It demonstrates:

- historical backfill;
- live tail;
- Sui gRPC checkpoint ingestion;
- raw event storage;
- normalized domain tables;
- SQLite and Postgres backends;
- REST API;
- Prometheus metrics;
- Walrus content enrichment;
- replay/rebuild;
- airdrop snapshot exports;
- deployment drift checks.

## What This Indexer Is Not

It is not the only valid PaperProof indexer.

It is not a mandatory service for using PaperProof Protocol.

It is not a final search, ranking, moderation, or reward policy.

Forks are encouraged, provided they keep official/unofficial identity clear and
preserve appropriate trust checks when claiming compatibility with official
PaperProof deployments.
