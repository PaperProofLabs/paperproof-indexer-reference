// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalyticsSummary {
    pub total_artifacts: u64,
    pub total_versions: u64,
    pub total_comments: u64,
    pub total_likes: u64,
    pub total_proposals: u64,
    pub total_votes: u64,
    pub last_checkpoint: Option<u64>,
    pub content_refs_pending: u64,
    pub content_cache_verified: u64,
    pub top_contributors: Vec<ContributorSummary>,
    pub artifact_types: Vec<ArtifactTypeSummary>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContributorSummary {
    pub address: String,
    pub score: u64,
    pub published_artifacts: u64,
    pub comments: u64,
    pub votes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactTypeSummary {
    pub artifact_type: u64,
    pub count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AirdropSnapshotPlan {
    pub name: String,
    pub description: String,
    pub source_tables: Vec<String>,
    pub output_columns: Vec<String>,
}

impl Default for AirdropSnapshotPlan {
    fn default() -> Self {
        Self {
            name: "paperproof-reference-snapshot".to_string(),
            description: "Reserved analytics layer for future PaperProof participation snapshots."
                .to_string(),
            source_tables: vec![
                "paperproof_events".to_string(),
                "paperproof_content_refs".to_string(),
                "domain_artifacts".to_string(),
                "domain_votes".to_string(),
            ],
            output_columns: vec![
                "address".to_string(),
                "published_artifacts".to_string(),
                "comments".to_string(),
                "votes".to_string(),
                "snapshot_reason".to_string(),
            ],
        }
    }
}
