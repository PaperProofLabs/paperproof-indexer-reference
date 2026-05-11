// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_trait::async_trait;
use paperproof_indexer_reference::{
    ContentEnrichmentInput, ContentEnrichmentStatus, ContentRef, InMemoryContentRefStore,
    PaperProofContentEnricher, WalrusContentSource,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Default)]
struct MockWalrus {
    blobs: BTreeMap<String, Vec<u8>>,
}

#[async_trait]
impl WalrusContentSource for MockWalrus {
    async fn read_blob(
        &self,
        blob_id: &str,
    ) -> paperproof_sdk_rs::Result<paperproof_sdk_rs::ContentReadResult> {
        let Some(bytes) = self.blobs.get(blob_id) else {
            return Err(paperproof_sdk_rs::PaperProofError::network(
                "mock-walrus",
                format!("missing blob {blob_id}"),
            ));
        };
        Ok(paperproof_sdk_rs::ContentReadResult {
            blob_id: blob_id.to_string(),
            bytes: bytes.clone(),
            digest: String::new(),
            verified: true,
        })
    }
}

#[tokio::test]
async fn enrichment_verifies_hash_and_preview() {
    let mut source = MockWalrus::default();
    source
        .blobs
        .insert("blob-ok".to_string(), b"hello paperproof".to_vec());
    let enricher = PaperProofContentEnricher::new(source, InMemoryContentRefStore::default());

    let output = enricher
        .enrich_one(ContentEnrichmentInput {
            content_ref: ContentRef {
                source_event_key: "event-1".to_string(),
                artifact_id: Some("series-1".to_string()),
                version_id: Some("version-1".to_string()),
                blob_id: "blob-ok".to_string(),
                expected_sha256_hex: Some(hex_sha256(b"hello paperproof")),
                content_type: Some("text/plain".to_string()),
            },
            max_bytes_to_keep: 5,
        })
        .await;

    assert_eq!(output.status, ContentEnrichmentStatus::Verified);
    assert_eq!(output.preview_utf8.as_deref(), Some("hello"));
    assert_eq!(output.byte_len, Some(16));
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tokio::test]
async fn enrichment_reports_digest_mismatch() {
    let mut source = MockWalrus::default();
    source.blobs.insert("blob-bad".to_string(), b"abc".to_vec());
    let enricher = PaperProofContentEnricher::new(source, InMemoryContentRefStore::default());
    let output = enricher
        .enrich_one(ContentEnrichmentInput {
            content_ref: ContentRef {
                source_event_key: "event-2".to_string(),
                artifact_id: None,
                version_id: None,
                blob_id: "blob-bad".to_string(),
                expected_sha256_hex: Some("deadbeef".to_string()),
                content_type: None,
            },
            max_bytes_to_keep: 10,
        })
        .await;
    assert_eq!(output.status, ContentEnrichmentStatus::DigestMismatch);
    assert!(output.sha256_hex.is_some());
}

#[tokio::test]
async fn enrichment_reports_fetch_failure() {
    let enricher =
        PaperProofContentEnricher::new(MockWalrus::default(), InMemoryContentRefStore::default());
    let output = enricher
        .enrich_one(ContentEnrichmentInput {
            content_ref: ContentRef {
                source_event_key: "event-3".to_string(),
                artifact_id: None,
                version_id: None,
                blob_id: "missing".to_string(),
                expected_sha256_hex: None,
                content_type: None,
            },
            max_bytes_to_keep: 10,
        })
        .await;
    assert_eq!(output.status, ContentEnrichmentStatus::FetchFailed);
    assert!(output.error.unwrap_or_default().contains("missing"));
}
