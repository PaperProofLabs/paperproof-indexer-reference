// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use paperproof_sdk_rs::{PaperProofContentBackend, PaperProofContentService, walrus::WalrusClient};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::store::{ContentRef, ContentRefStore};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContentEnrichmentInput {
    pub content_ref: ContentRef,
    pub max_bytes_to_keep: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentEnrichmentStatus {
    Verified,
    DigestMismatch,
    FetchFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContentEnrichmentOutput {
    pub source_event_key: String,
    pub blob_id: String,
    pub status: ContentEnrichmentStatus,
    pub sha256_hex: Option<String>,
    pub byte_len: Option<usize>,
    pub content_type: Option<String>,
    pub preview_utf8: Option<String>,
    pub error: Option<String>,
}

#[async_trait]
pub trait WalrusContentSource: Send + Sync {
    async fn read_blob(
        &self,
        blob_id: &str,
    ) -> paperproof_sdk_rs::Result<paperproof_sdk_rs::ContentReadResult>;
}

#[async_trait]
impl<B> WalrusContentSource for PaperProofContentService<B>
where
    B: PaperProofContentBackend + Send + Sync,
{
    async fn read_blob(
        &self,
        blob_id: &str,
    ) -> paperproof_sdk_rs::Result<paperproof_sdk_rs::ContentReadResult> {
        self.read_content(blob_id, None).await
    }
}

pub struct PaperProofContentEnricher<S, Store> {
    pub source: S,
    pub store: Store,
}

impl<S, Store> PaperProofContentEnricher<S, Store>
where
    S: WalrusContentSource,
    Store: ContentRefStore,
{
    pub fn new(source: S, store: Store) -> Self {
        Self { source, store }
    }

    pub async fn enrich_pending(
        &self,
        limit: usize,
        max_bytes_to_keep: usize,
    ) -> paperproof_sdk_rs::Result<Vec<ContentEnrichmentOutput>> {
        let refs = self.store.list_pending_refs(limit).await?;
        let mut outputs = Vec::with_capacity(refs.len());
        for content_ref in refs {
            let output = self
                .enrich_one(ContentEnrichmentInput {
                    content_ref,
                    max_bytes_to_keep,
                })
                .await;
            self.store
                .mark_enriched(
                    &output.source_event_key,
                    status_label(&output.status),
                    serde_json::to_value(&output)?,
                )
                .await?;
            outputs.push(output);
        }
        Ok(outputs)
    }

    pub async fn enrich_one(&self, input: ContentEnrichmentInput) -> ContentEnrichmentOutput {
        let content_ref = input.content_ref;
        match self.source.read_blob(&content_ref.blob_id).await {
            Ok(read) => {
                let sha256_hex = sha256_hex(&read.bytes);
                let verified = content_ref
                    .expected_sha256_hex
                    .as_deref()
                    .map(|expected| expected.eq_ignore_ascii_case(&sha256_hex))
                    .unwrap_or(true);
                ContentEnrichmentOutput {
                    source_event_key: content_ref.source_event_key,
                    blob_id: content_ref.blob_id,
                    status: if verified {
                        ContentEnrichmentStatus::Verified
                    } else {
                        ContentEnrichmentStatus::DigestMismatch
                    },
                    sha256_hex: Some(sha256_hex),
                    byte_len: Some(read.bytes.len()),
                    content_type: content_ref.content_type,
                    preview_utf8: utf8_preview(&read.bytes, input.max_bytes_to_keep),
                    error: None,
                }
            }
            Err(error) => ContentEnrichmentOutput {
                source_event_key: content_ref.source_event_key,
                blob_id: content_ref.blob_id,
                status: ContentEnrichmentStatus::FetchFailed,
                sha256_hex: None,
                byte_len: None,
                content_type: content_ref.content_type,
                preview_utf8: None,
                error: Some(error.to_string()),
            },
        }
    }
}

pub fn default_walrus_content_service(
    aggregator_url: impl Into<String>,
) -> PaperProofContentService<WalrusClient> {
    PaperProofContentService::new(WalrusClient::new(aggregator_url, None))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn utf8_preview(bytes: &[u8], max_bytes: usize) -> Option<String> {
    let end = bytes.len().min(max_bytes);
    std::str::from_utf8(&bytes[..end])
        .ok()
        .map(|text| text.to_string())
}

fn status_label(status: &ContentEnrichmentStatus) -> &'static str {
    match status {
        ContentEnrichmentStatus::Verified => "verified",
        ContentEnrichmentStatus::DigestMismatch => "digest_mismatch",
        ContentEnrichmentStatus::FetchFailed => "fetch_failed",
    }
}
