// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "sqlite")]

use paperproof_indexer_reference::{
    ApiConfig, ApiState, NormalizedQuery, build_event_sink, run_api_server,
    site_analytics::SiteAnalyticsConfig,
};
use paperproof_sdk_rs::{
    EventId, IndexedPaperProofEvent, IndexerEventBatch, IndexerProgress, PaperProofEventSink,
    events::{PaperProofEventKind, SuiEventEnvelope},
    events_trust::{
        EventTrustResult, EventVerificationStatus, verification_report_from_canonical_check,
    },
};
use serde_json::{Value, json};
use std::time::Duration;

async fn wait_for_json(client: &reqwest::Client, url: &str) -> Value {
    let mut last_error = String::new();
    for _ in 0..40 {
        match client.get(url).send().await {
            Ok(response) => {
                let status = response.status();
                match response.text().await {
                    Ok(body) if status.is_success() && !body.trim().is_empty() => {
                        match serde_json::from_str::<Value>(&body) {
                            Ok(value) => return value,
                            Err(error) => {
                                last_error =
                                    format!("invalid json from {url}: {error}; body={body:?}");
                            }
                        }
                    }
                    Ok(body) => {
                        last_error = format!(
                            "non-ready response from {url}: status={status}; body={body:?}"
                        );
                    }
                    Err(error) => {
                        last_error = format!("failed to read response from {url}: {error}");
                    }
                }
            }
            Err(error) => {
                last_error = format!("request failed for {url}: {error}");
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not return JSON in time: {last_error}");
}

#[tokio::test]
async fn api_serves_health_metrics_and_search() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("api.sqlite");
    let _ = NormalizedQuery::sqlite(db.to_string_lossy())
        .summary()
        .expect("init schema");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);

    let bind = addr.to_string();
    let db_path = db.to_string_lossy().to_string();
    let handle = tokio::spawn(async move {
        let _ = run_api_server(
            ApiConfig { bind },
            ApiState {
                sqlite_path: Some(db_path),
                ..Default::default()
            },
        )
        .await;
    });
    let base = format!("http://{addr}");
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("reqwest client");
    let health = wait_for_json(&client, &format!("{base}/health")).await;
    assert_eq!(health["status"], "ok");

    let metrics = wait_for_json(&client, &format!("{base}/metrics")).await;
    assert_eq!(metrics["database_ready"], true);

    let search = wait_for_json(&client, &format!("{base}/v1/search/artifacts?q=none")).await;
    assert_eq!(search.as_array().expect("search array").len(), 0);

    let prometheus = client
        .get(format!("{base}/metrics/prometheus"))
        .send()
        .await
        .expect("prometheus request")
        .text()
        .await
        .expect("prometheus text");
    assert!(prometheus.contains("paperproof_indexer_processed_events_total"));

    handle.abort();
}

#[tokio::test]
async fn api_serves_projected_artifacts_governance_and_airdrop() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().to_string_lossy().to_string();
    let sink = build_event_sink("sqlite", &out, "api-flow")
        .await
        .expect("sqlite sink");
    sink.write_batch(&IndexerEventBatch {
        accepted: vec![
            event(
                1,
                PaperProofEventKind::ArtifactPublished,
                "ArtifactPublishedEvent",
                json!({
                    "series_id": "0xapi_series",
                    "version_id": "0xapi_version_1",
                    "artifact_code": "PaperProof-preprint-api-test",
                    "artifact_type": 1,
                    "author": "0xapi_author",
                    "comments_tree_id": "0xapi_tree",
                    "likes_book_id": "0xapi_likes",
                    "content_hash": "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
                    "walrus_blob_id": "api-blob-v1",
                    "content_type": "text/plain",
                    "version": 1
                }),
            ),
            event(
                2,
                PaperProofEventKind::ArtifactVersionAdded,
                "ArtifactVersionAddedEvent",
                json!({
                    "series_id": "0xapi_series",
                    "version_id": "0xapi_version_2",
                    "artifact_type": 1,
                    "version": 2,
                    "content_hash": "sha256:486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7",
                    "walrus_blob_id": "api-blob-v2",
                    "content_type": "text/plain"
                }),
            ),
            event(
                3,
                PaperProofEventKind::CommentAdded,
                "CommentAddedEvent",
                json!({
                    "tree_id": "0xapi_tree",
                    "comment_id": 11,
                    "parent_comment_id": 0,
                    "commenter": "0xapi_commenter",
                    "content_mode": 0
                }),
            ),
            event(
                4,
                PaperProofEventKind::ProposalCreated,
                "ProposalCreatedEvent",
                json!({
                    "proposal_id": 9,
                    "proposal_object_id": "0xapi_proposal",
                    "proposer": "0xapi_author",
                    "title": "API proposal",
                    "proposal_type": 1,
                    "status": 1
                }),
            ),
            event(
                5,
                PaperProofEventKind::ProposalVoted,
                "VoteCastEvent",
                json!({
                    "proposal_id": 9,
                    "voter": "0xapi_voter",
                    "side": 1,
                    "voting_power": "123"
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
    let (base, handle) = spawn_api(db.to_string_lossy().to_string()).await;
    let client = local_client();
    let _ = wait_for_json(&client, &format!("{base}/health")).await;

    let explore = wait_for_json(&client, &format!("{base}/v1/explore/artifacts?limit=10")).await;
    assert_eq!(explore.as_array().expect("explore array").len(), 1);
    assert_eq!(explore[0]["latest_version_id"], "0xapi_version_2");

    let search = wait_for_json(
        &client,
        &format!("{base}/v1/search/artifacts?q=preprint&artifact_type=1&owner=0xapi_author"),
    )
    .await;
    assert_eq!(search.as_array().expect("search array").len(), 1);

    let detail = wait_for_json(&client, &format!("{base}/v1/artifacts/0xapi_series")).await;
    assert_eq!(
        detail["artifact"]["artifact_code"],
        "PaperProof-preprint-api-test"
    );
    assert_eq!(detail["versions"].as_array().expect("versions").len(), 2);
    assert_eq!(detail["comments"].as_array().expect("comments").len(), 1);

    let activity = wait_for_json(
        &client,
        &format!("{base}/v1/activity?series_id=0xapi_series&limit=10"),
    )
    .await;
    assert!(
        activity
            .as_array()
            .expect("activity array")
            .iter()
            .any(|item| item["kind"] == "ArtifactPublished")
    );

    let proposals = wait_for_json(&client, &format!("{base}/v1/governance/proposals")).await;
    assert_eq!(proposals[0]["title"], "API proposal");

    let votes = wait_for_json(&client, &format!("{base}/v1/my/0xapi_voter/votes")).await;
    assert_eq!(votes[0]["voting_power"], "123");

    let airdrop = wait_for_json(&client, &format!("{base}/v1/airdrop/snapshot")).await;
    assert!(
        airdrop
            .as_array()
            .expect("airdrop array")
            .iter()
            .any(|row| row["address"] == "0xapi_voter" && row["votes"] == 1)
    );

    handle.abort();
}

#[tokio::test]
async fn site_analytics_is_disabled_by_default_and_hashes_enabled_visits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("analytics.sqlite");
    let _ = NormalizedQuery::sqlite(db.to_string_lossy())
        .summary()
        .expect("init schema");
    let (base, handle) = spawn_api(db.to_string_lossy().to_string()).await;
    let client = local_client();
    let disabled = client
        .post(format!("{base}/v1/site-analytics/visit"))
        .json(&json!({
            "visitorId": "visitor-a",
            "path": "/#/explore",
            "language": "en-US"
        }))
        .send()
        .await
        .expect("disabled visit request")
        .json::<Value>()
        .await
        .expect("disabled visit json");
    assert_eq!(disabled["recorded"], false);
    handle.abort();

    let db_path = db.to_string_lossy().to_string();
    let (base, handle) = spawn_api_with_state(ApiState {
        sqlite_path: Some(db_path.clone()),
        site_analytics: SiteAnalyticsConfig {
            enabled: true,
            salt: Some("test-salt".to_string()),
            admin_token: Some("admin-token".to_string()),
        },
        ..Default::default()
    })
    .await;
    let _ = wait_for_json(&client, &format!("{base}/health")).await;
    for visitor in ["visitor-a", "visitor-b"] {
        let response = client
            .post(format!("{base}/v1/site-analytics/visit"))
            .header("x-real-ip", "203.0.113.10")
            .header("user-agent", "PaperProof Test Browser")
            .header("accept-language", "en-US")
            .json(&json!({
                "visitorId": visitor,
                "path": "/#/explore",
                "language": "en-US",
                "timezone": "UTC",
                "screen": "1200x800",
                "platform": "test"
            }))
            .send()
            .await
            .expect("enabled visit request")
            .json::<Value>()
            .await
            .expect("enabled visit json");
        assert_eq!(response["recorded"], true);
    }
    let weekly = client
        .get(format!("{base}/v1/site-analytics/weekly"))
        .bearer_auth("admin-token")
        .send()
        .await
        .expect("weekly request")
        .json::<Value>()
        .await
        .expect("weekly json");
    assert_eq!(weekly["visits"], 2);
    assert_eq!(weekly["uniqueVisitors"], 2);
    assert_eq!(weekly["uniqueIps"], 1);

    let conn = rusqlite::Connection::open(&db_path).expect("open analytics db");
    let raw_matches: i64 = conn
        .query_row(
            "select count(*) from site_visit_events
             where visitor_id_hash in ('visitor-a', 'visitor-b')
                or ip_hash = '203.0.113.10'",
            [],
            |row| row.get(0),
        )
        .expect("raw check");
    assert_eq!(raw_matches, 0);
    handle.abort();
}

async fn spawn_api(db_path: String) -> (String, tokio::task::JoinHandle<()>) {
    spawn_api_with_state(ApiState {
        sqlite_path: Some(db_path),
        ..Default::default()
    })
    .await
}

async fn spawn_api_with_state(state: ApiState) -> (String, tokio::task::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);

    let bind = addr.to_string();
    let handle = tokio::spawn(async move {
        let _ = run_api_server(ApiConfig { bind }, state).await;
    });
    (format!("http://{addr}"), handle)
}

fn local_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("reqwest client")
}

fn event(
    seq: u64,
    kind: PaperProofEventKind,
    struct_name: &str,
    parsed_json: Value,
) -> IndexedPaperProofEvent {
    let event = SuiEventEnvelope {
        id: Some(json!({ "txDigest": format!("api-digest-{seq}"), "eventSeq": seq })),
        package_id: paperproof_sdk_rs::MAINNET_DEPLOYMENT
            .packages
            .publishing
            .clone(),
        transaction_module: "publishing".to_string(),
        sender: parsed_json
            .get("author")
            .or_else(|| parsed_json.get("commenter"))
            .or_else(|| parsed_json.get("voter"))
            .or_else(|| parsed_json.get("proposer"))
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
            transaction_digest: Some(format!("api-digest-{seq}")),
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
