// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use paperproof_sdk_rs::{
    EventQueryInput, IndexerEventBatch, IndexerProgress, IndexerScanOptions, IndexerTrustPolicy,
    PaginationInput, PaperProofEventSink, PaperProofIndexerClient, StoredIndexerCursor, StreamId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BackfillReport {
    pub pages_scanned: u64,
    pub accepted_events: u64,
    pub rejected_events: u64,
    pub accepted_written: usize,
    pub rejected_written: usize,
    pub duplicate_skipped: usize,
    pub trust_policy: IndexerTrustPolicy,
}

pub type TailReport = BackfillReport;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplayReport {
    pub input_path: String,
    pub events_seen: u64,
    pub domain_events_applied: u64,
    pub output_path: Option<String>,
}

pub async fn run_backfill_once(
    indexer: &PaperProofIndexerClient,
    sink: &dyn PaperProofEventSink,
    cursor_store: &dyn paperproof_sdk_rs::IndexerCursorStore,
    page_limit: u64,
    pages: u64,
    trust_policy: IndexerTrustPolicy,
    fail_on_rejected: bool,
) -> paperproof_sdk_rs::Result<BackfillReport> {
    let mut report = BackfillReport {
        trust_policy: trust_policy.clone(),
        ..Default::default()
    };
    for module in PaperProofIndexerClient::canonical_module_filters(&indexer.query.deployment) {
        let stream = StreamId::from(&module);
        let mut cursor = cursor_store
            .load_cursor(&stream)
            .await?
            .and_then(|stored| stored.event_cursor);
        for _ in 0..pages {
            let batch = indexer
                .scan_once(IndexerScanOptions {
                    filter: EventQueryInput {
                        package_id: Some(module.package_id.clone()),
                        module: Some(module.module.clone()),
                        pagination: PaginationInput {
                            cursor: cursor.clone(),
                            limit: Some(page_limit),
                            descending_order: Some(false),
                        },
                        ..Default::default()
                    },
                    canonical_only: trust_policy != IndexerTrustPolicy::Raw,
                    trust_policy: trust_policy.clone(),
                })
                .await?;
            assert_batch_acceptable(&batch, fail_on_rejected)?;
            cursor = durable_event_cursor(&batch, cursor.clone());
            let summary = sink.write_batch(&batch).await?;
            cursor_store
                .save_cursor(
                    &stream,
                    StoredIndexerCursor {
                        event_cursor: cursor.clone(),
                        checkpoint_cursor: None,
                    },
                )
                .await?;
            report.pages_scanned += 1;
            report.accepted_events += batch.progress.accepted_events;
            report.rejected_events += batch.progress.rejected_events;
            report.accepted_written += summary.accepted_written;
            report.rejected_written += summary.rejected_written;
            report.duplicate_skipped += summary.duplicate_skipped;
            if !batch.progress.has_next_page {
                break;
            }
        }
    }
    Ok(report)
}

pub fn replay_jsonl_to_state(
    input_path: &str,
    output_path: Option<&str>,
) -> paperproof_sdk_rs::Result<ReplayReport> {
    let text = std::fs::read_to_string(input_path)
        .map_err(|err| paperproof_sdk_rs::PaperProofError::network(input_path, err.to_string()))?;
    let mut state = paperproof_sdk_rs::PaperProofIndexerState::default();
    let mut events_seen = 0;
    let mut domain_events_applied = 0;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        events_seen += 1;
        let event: paperproof_sdk_rs::IndexedPaperProofEvent = serde_json::from_str(line)?;
        state.apply_event(&event);
        domain_events_applied += 1;
    }
    if let Some(path) = output_path {
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|err| {
                paperproof_sdk_rs::PaperProofError::network(
                    parent.display().to_string(),
                    err.to_string(),
                )
            })?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&state)?)
            .map_err(|err| paperproof_sdk_rs::PaperProofError::network(path, err.to_string()))?;
    }
    Ok(ReplayReport {
        input_path: input_path.to_string(),
        events_seen,
        domain_events_applied,
        output_path: output_path.map(ToString::to_string),
    })
}

pub async fn run_tail_once(
    indexer: &PaperProofIndexerClient,
    sink: &dyn PaperProofEventSink,
    cursor_store: &dyn paperproof_sdk_rs::IndexerCursorStore,
    page_limit: u64,
    trust_policy: IndexerTrustPolicy,
    fail_on_rejected: bool,
) -> paperproof_sdk_rs::Result<TailReport> {
    let mut report = TailReport {
        trust_policy: trust_policy.clone(),
        ..Default::default()
    };
    for module in PaperProofIndexerClient::canonical_module_filters(&indexer.query.deployment) {
        let stream = StreamId::from(&module);
        let previous_cursor = cursor_store
            .load_cursor(&stream)
            .await?
            .and_then(|stored| stored.event_cursor);
        let progress = previous_cursor.clone().map(|cursor| IndexerProgress {
            cursor: Some(cursor),
            ..Default::default()
        });
        let batch = indexer
            .scan_once(IndexerScanOptions {
                filter: EventQueryInput {
                    package_id: Some(module.package_id),
                    module: Some(module.module),
                    pagination: PaginationInput {
                        cursor: progress.and_then(|progress| progress.cursor),
                        limit: Some(page_limit),
                        descending_order: Some(false),
                    },
                    ..Default::default()
                },
                canonical_only: trust_policy != IndexerTrustPolicy::Raw,
                trust_policy: trust_policy.clone(),
            })
            .await?;
        assert_batch_acceptable(&batch, fail_on_rejected)?;
        let summary = sink.write_batch(&batch).await?;
        cursor_store
            .save_cursor(
                &stream,
                StoredIndexerCursor {
                    event_cursor: durable_event_cursor(&batch, previous_cursor),
                    checkpoint_cursor: None,
                },
            )
            .await?;
        report.pages_scanned += 1;
        report.accepted_events += batch.progress.accepted_events;
        report.rejected_events += batch.progress.rejected_events;
        report.accepted_written += summary.accepted_written;
        report.rejected_written += summary.rejected_written;
        report.duplicate_skipped += summary.duplicate_skipped;
    }
    Ok(report)
}

fn durable_event_cursor(batch: &IndexerEventBatch, previous: Option<Value>) -> Option<Value> {
    batch
        .progress
        .cursor
        .clone()
        .or_else(|| graphql_page_cursor(&batch.raw))
        .or(previous)
}

fn graphql_page_cursor(raw: &Value) -> Option<Value> {
    let page_info = raw.pointer("/data/events/pageInfo")?;
    page_info
        .get("endCursor")
        .cloned()
        .filter(|value| !value.is_null())
        .or_else(|| {
            page_info
                .get("startCursor")
                .cloned()
                .filter(|value| !value.is_null())
        })
}

fn assert_batch_acceptable(
    batch: &IndexerEventBatch,
    fail_on_rejected: bool,
) -> paperproof_sdk_rs::Result<()> {
    if fail_on_rejected && batch.progress.rejected_events > 0 {
        return Err(paperproof_sdk_rs::PaperProofError::event_verification(
            format!(
                "PaperProof indexer batch has {} rejected event(s); refusing to persist because fail_on_rejected=true",
                batch.progress.rejected_events
            ),
        ));
    }
    Ok(())
}
