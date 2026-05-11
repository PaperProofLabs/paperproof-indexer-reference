# Walrus Enrichment

PaperProof artifacts often reference Walrus blobs. The indexer treats Walrus
content as a second-stage enrichment pipeline, separate from Sui fact indexing.

This separation is important:

- Sui events establish protocol facts;
- Walrus content fetches can be slow or temporarily unavailable;
- content preview extraction can be CPU or IO intensive;
- gateway failures should not block chain indexing.

## Data Flow

```text
ArtifactPublished / ArtifactVersionAdded event
        |
        v
domain_versions
        |
        v
paperproof_content_refs
        |
        v
enrich-content worker
        |
        v
Walrus aggregator
        |
        v
hash verification + preview extraction
        |
        v
paperproof_content_cache
```

## Content References

The normalized reducer inserts `paperproof_content_refs` when an artifact
version contains:

- `walrus_blob_id` or `blob_id`;
- optional `content_hash`;
- optional `content_type`.

If `content_hash` starts with `sha256:`, the prefix is stripped and stored as
`expected_sha256_hex`.

## Enrichment Command

```bash
cargo run --features sqlite -- enrich-content \
  --sqlite-path artifacts/indexer-mainnet/paperproof-indexer-reference.sqlite \
  --walrus-aggregator-url https://aggregator.walrus.space \
  --limit 25 \
  --max-preview-bytes 4096
```

The worker:

1. selects pending or fetch-failed refs;
2. reads blob bytes;
3. computes SHA-256;
4. compares against expected hash when available;
5. stores status and details;
6. stores UTF-8 preview when content is valid UTF-8.

## Status Values

`verified`

Blob was fetched and either matched the expected hash or no expected hash was
available.

`digest_mismatch`

Blob was fetched but SHA-256 did not match the expected hash.

`fetch_failed`

Blob could not be fetched.

`pending`

Blob has not been enriched yet.

## What Is Stored

`paperproof_content_cache` stores:

- blob id;
- calculated SHA-256;
- byte length;
- content type;
- UTF-8 preview;
- status;
- error text;
- updated time.

The current implementation stores preview text only. It does not store full
files or extracted PDF text.

## Recommended Production Enhancements

For a production Explorer or search engine, add:

- concurrent workers;
- retry with exponential backoff;
- per-aggregator health checks;
- blob bytes in object storage;
- MIME sniffing;
- PDF text extraction;
- Markdown rendering;
- dataset manifest parsing;
- antivirus or content safety checks if needed;
- full-text indexing into Postgres FTS, Meilisearch, or Elastic.

## Failure Handling

Do not block Sui indexing on Walrus fetch failures.

Frontends should distinguish:

- no content reference;
- content reference pending;
- fetch failed;
- digest mismatch;
- verified and preview available.

Digest mismatch is important and should be visible to operators. It means the
indexed chain fact and fetched content do not match.
