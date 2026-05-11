# Third-Party Extensions

This repository is designed to be forked. It is a reference implementation, not
the only correct PaperProof indexer.

Third-party teams can build:

- academic search engines;
- type-specific PaperProof websites;
- community forums;
- dataset explorers;
- analytics dashboards;
- airdrop review tools;
- recommendation systems;
- moderation systems;
- archival services;
- custom APIs.

## Safe Extension Points

### Event Source

Replace or add:

- Sui GraphQL event queries;
- Sui gRPC checkpoint ingestion;
- archival providers;
- self-hosted Sui nodes;
- third-party data providers.

Keep canonical deployment checks in place.

### Sinks

Replace or add:

- Postgres;
- SQLite;
- ClickHouse;
- BigQuery;
- S3/object storage;
- Kafka;
- Redpanda;
- Parquet files.

Preserve idempotent event keys if you need replayable raw facts.

### Reducers

Add custom reducers for:

- moderation state;
- artifact ranking;
- author reputation;
- citation graph;
- topic feeds;
- social feeds;
- governance dashboards;
- airdrop scoring;
- search documents.

Keep raw events separate from derived state.

### Search

The reference search is intentionally simple. Production search can use:

- Postgres full-text search;
- trigram indexes;
- Meilisearch;
- Elastic;
- OpenSearch;
- Tantivy;
- custom embedding/vector search.

Search documents may combine:

- artifact metadata;
- version metadata;
- Walrus preview text;
- extracted PDF text;
- comment content;
- citation metadata;
- author metadata.

### Walrus Enrichment

Extend content handling with:

- multi-aggregator fallback;
- concurrent fetches;
- retry queues;
- PDF extraction;
- Markdown rendering;
- dataset manifests;
- media metadata;
- virus scanning;
- object-storage cache.

### API

Replace REST with:

- GraphQL;
- gRPC;
- tRPC;
- app-specific BFF;
- static JSON exports.

The normalized tables are meant to make this easy.

## What Not To Remove

If your indexer claims compatibility with official PaperProof deployments, do
not remove:

- deployment manifest checks;
- canonical package/module filtering;
- rejected event tracking;
- raw event persistence;
- cursor persistence;
- clear distinction between failed query and empty result.

## Official Identity

You may fork and modify this reference. Do not present a modified fork as the
official PaperProof Labs indexer unless PaperProof Labs explicitly authorizes
it.

Make modified status clear in:

- README;
- API headers or metadata;
- deployment docs;
- frontend footer;
- repository description.

## Recommended Fork Strategy

1. Keep raw event tables compatible.
2. Add your own normalized tables instead of mutating core ones heavily.
3. Keep replay commands working.
4. Add migration scripts for new tables.
5. Add integration tests for custom reducers.
6. Document trust assumptions.
7. Expose clear API errors.
8. Monitor rejected event counts.

## Example Extensions

### Explorer

Add:

- type filters;
- full-text search;
- artifact timeline;
- author pages;
- citation graph;
- Walrus preview/download.

### Airdrop Tooling

Add:

- rule manifests;
- score explanations;
- address exclusions;
- snapshot checkpoint range;
- CSV/JSON signatures;
- review UI.

### Academic Search

Add:

- PDF extraction;
- keywords;
- abstract parsing;
- topic embeddings;
- citation ranking;
- author graph.

### Community Forum

Add:

- selected artifact types;
- comment tree views;
- moderation state;
- activity feed;
- likes/dislikes;
- community-specific ranking.
