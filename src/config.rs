// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use paperproof_sdk_rs::IndexerTrustPolicy;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkName {
    #[default]
    Mainnet,
    Testnet,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReferenceIndexerConfig {
    pub network: NetworkName,
    pub sink: String,
    pub output_dir: String,
    pub page_limit: u64,
    pub pages: u64,
    pub trust_policy: IndexerTrustPolicy,
    pub fail_on_rejected: bool,
    pub tail_interval_ms: u64,
    pub walrus_aggregator_url: String,
}

impl Default for ReferenceIndexerConfig {
    fn default() -> Self {
        Self {
            network: NetworkName::Mainnet,
            sink: "jsonl".to_string(),
            output_dir: "artifacts/indexer".to_string(),
            page_limit: 50,
            pages: 1,
            trust_policy: IndexerTrustPolicy::Canonical,
            fail_on_rejected: true,
            tail_interval_ms: 10_000,
            walrus_aggregator_url: "https://aggregator.walrus-testnet.walrus.space".to_string(),
        }
    }
}
