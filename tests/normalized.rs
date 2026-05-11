// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_normalized_schema_is_available() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("normalized.sqlite");
    let query = paperproof_indexer_reference::NormalizedQuery::sqlite(db.to_string_lossy());
    let summary = query.summary().expect("summary");
    assert_eq!(summary.total_artifacts, 0);
    assert_eq!(summary.total_votes, 0);
}

#[test]
fn postgres_schema_exposes_normalized_projection_and_metrics_tables() {
    let schema = paperproof_indexer_reference::POSTGRES_REFERENCE_SCHEMA;
    for table in [
        "domain_artifacts",
        "domain_versions",
        "domain_comments",
        "domain_governance_proposals",
        "domain_votes",
        "domain_activity",
        "domain_airdrop_scores",
        "paperproof_indexer_metrics",
        "paperproof_indexer_metric_samples",
    ] {
        assert!(schema.contains(table), "missing {table}");
    }
    assert!(schema.contains("domain_activity_actor_idx"));
    assert!(schema.contains("paperproof_content_cache"));
}
