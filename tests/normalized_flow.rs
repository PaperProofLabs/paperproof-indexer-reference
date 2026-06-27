// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "sqlite")]

use paperproof_indexer_reference::{
    ContentRefStore, NormalizedQuery, SqliteContentRefStore, build_event_sink,
    rebuild_normalized_from_sqlite_raw,
};
use paperproof_sdk_rs::{
    EventId, IndexedPaperProofEvent, IndexerEventBatch, IndexerProgress, PaperProofEventSink,
    events::{PaperProofEventKind, SuiEventEnvelope},
    events_trust::{
        EventTrustResult, EventVerificationStatus, verification_report_from_canonical_check,
    },
};
use serde_json::{Value, json};

#[tokio::test]
async fn sqlite_sink_projects_normalized_views_and_airdrop_scores() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().to_string_lossy().to_string();
    let sink = build_event_sink("sqlite", &out, "test")
        .await
        .expect("sink");
    sink.write_batch(&IndexerEventBatch {
        accepted: vec![
            event(
                1,
                PaperProofEventKind::ArtifactPublished,
                "ArtifactPublishedEvent",
                json!({
                    "series_id": "0xseries",
                    "version_id": "0xversion1",
                    "artifact_code": "PaperProof-preprint-000001-test",
                    "artifact_type": 1,
                    "author": "0xauthor",
                    "comments_tree_id": "0xtree",
                    "likes_book_id": "0xlikes",
                    "content_hash": "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
                    "walrus_blob_id": "blob-v1",
                    "content_type": "text/plain",
                    "version": 1
                }),
            ),
            event(
                2,
                PaperProofEventKind::ArtifactVersionAdded,
                "ArtifactVersionAddedEvent",
                json!({
                    "series_id": "0xseries",
                    "version_id": "0xversion2",
                    "artifact_type": 1,
                    "version": 2,
                    "content_hash": "sha256:486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7",
                    "walrus_blob_id": "blob-v2",
                    "content_type": "text/plain"
                }),
            ),
            event(
                3,
                PaperProofEventKind::ArtifactStatusChanged,
                "ArtifactStatusChangedEvent",
                json!({
                    "series_id": "0xseries",
                    "changed_by": "0xoperator",
                    "old_status": 0,
                    "new_status": 2
                }),
            ),
            event(
                4,
                PaperProofEventKind::CommentAdded,
                "CommentAddedEvent",
                json!({
                    "tree_id": "0xtree",
                    "comment_id": 1,
                    "parent_comment_id": 0,
                    "commenter": "0xcommenter",
                    "content_mode": 0
                }),
            ),
            event(
                5,
                PaperProofEventKind::ProposalVoted,
                "VoteCastEvent",
                json!({
                    "proposal_id": 7,
                    "voter": "0xvoter",
                    "side": 1,
                    "voting_power": "42"
                }),
            ),
            event(
                6,
                PaperProofEventKind::VoteClaimed,
                "VoteClaimedEvent",
                json!({
                    "proposal_id": 7,
                    "voter": "0xvoter"
                }),
            ),
        ],
        rejected: vec![],
        progress: IndexerProgress::default(),
        raw: Value::Null,
    })
    .await
    .expect("write batch");

    let db = dir.path().join("paperproof-indexer-reference.sqlite");
    let query = NormalizedQuery::sqlite(db.to_string_lossy());
    let summary = query.summary().expect("summary");
    assert_eq!(summary.total_artifacts, 1);
    assert_eq!(summary.total_versions, 2);
    assert_eq!(summary.total_comments, 1);
    assert_eq!(summary.total_votes, 1);
    assert_eq!(summary.content_refs_pending, 2);

    let artifact = query.artifact_detail("0xseries").expect("detail").unwrap();
    assert_eq!(artifact.latest_version_id.as_deref(), Some("0xversion2"));
    assert_eq!(artifact.comments_tree_id.as_deref(), Some("0xtree"));
    assert_eq!(artifact.status, Some(2));

    let search = query
        .search_artifacts("preprint", Some(1), Some("0xauthor"), 10, 0)
        .expect("search artifacts");
    assert_eq!(search.len(), 1);

    let activity = query
        .activity(None, Some("0xseries"), 10, 0)
        .expect("activity");
    assert!(activity.iter().any(|item| item.kind == "ArtifactPublished"));

    let comments = query.comments("0xseries", 25, 0).expect("comments");
    assert_eq!(comments[0].author.as_deref(), Some("0xcommenter"));

    let votes = query.votes_for_address("0xvoter", 5, 0).expect("votes");
    assert_eq!(votes[0].voting_power.as_deref(), Some("42"));
    assert!(votes[0].claimed);

    let airdrop = query.airdrop_rows().expect("airdrop");
    assert!(
        airdrop
            .iter()
            .any(|row| row.address == "0xvoter" && row.votes == 1)
    );

    let content_store = SqliteContentRefStore::new(db.to_string_lossy()).expect("content store");
    let refs = content_store
        .list_pending_refs(10)
        .await
        .expect("pending refs");
    assert_eq!(refs.len(), 2);
    content_store
        .mark_enriched(
            &refs[0].source_event_key,
            "verified",
            json!({
                "blob_id": refs[0].blob_id,
                "sha256_hex": "abc",
                "byte_len": 3,
                "content_type": "text/plain",
                "preview_utf8": "hey",
                "error": null
            }),
        )
        .await
        .expect("mark enriched");
    let summary = query.summary().expect("summary after cache");
    assert_eq!(summary.content_cache_verified, 1);

    let rebuild = rebuild_normalized_from_sqlite_raw(&db.to_string_lossy(), true)
        .expect("rebuild normalized");
    assert_eq!(rebuild.events_seen, 6);
    assert_eq!(rebuild.events_applied, 6);
    let summary = query.summary().expect("summary after rebuild");
    assert_eq!(summary.total_artifacts, 1);
    assert_eq!(summary.total_versions, 2);
}

fn event(
    seq: u64,
    kind: PaperProofEventKind,
    struct_name: &str,
    parsed_json: Value,
) -> IndexedPaperProofEvent {
    let event = SuiEventEnvelope {
        id: Some(json!({ "txDigest": format!("digest-{seq}"), "eventSeq": seq })),
        package_id: paperproof_sdk_rs::MAINNET_DEPLOYMENT
            .packages
            .publishing
            .clone(),
        transaction_module: "publishing".to_string(),
        sender: parsed_json
            .get("author")
            .or_else(|| parsed_json.get("commenter"))
            .or_else(|| parsed_json.get("voter"))
            .and_then(Value::as_str)
            .unwrap_or("0xsender")
            .to_string(),
        event_type: format!(
            "{}::publishing::{struct_name}",
            paperproof_sdk_rs::MAINNET_DEPLOYMENT.packages.publishing
        ),
        parsed_json,
        bcs: None,
        timestamp_ms: Some("2026-05-11T00:00:00Z".to_string()),
    };
    IndexedPaperProofEvent {
        id: EventId {
            checkpoint: Some(seq),
            transaction_digest: Some(format!("digest-{seq}")),
            event_seq: Some(seq),
            package_id: event.package_id.clone(),
            module: event.transaction_module.clone(),
            event_type: event.event_type.clone(),
        },
        verification: verification_report_from_canonical_check(
            &event,
            &paperproof_sdk_rs::MAINNET_DEPLOYMENT,
            paperproof_sdk_rs::EventTrustLevel::Canonical,
        ),
        trust: EventTrustResult {
            trusted: true,
            reason: None,
            status: EventVerificationStatus::Canonical,
            issues: vec![],
        },
        kind,
        event,
    }
}
