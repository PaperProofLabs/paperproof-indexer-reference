// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use paperproof_indexer_reference::ReferenceIndexerConfig;
use paperproof_sdk_rs::IndexerTrustPolicy;

#[test]
fn defaults_to_canonical_with_rejected_batches_blocked() {
    let config = ReferenceIndexerConfig::default();
    assert_eq!(config.trust_policy, IndexerTrustPolicy::Canonical);
    assert!(config.fail_on_rejected);
}
