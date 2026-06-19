// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::analytics::{AirdropSnapshotPlan, AnalyticsSummary};
use crate::metrics::IndexerMetricSnapshot;
#[cfg(feature = "sqlite")]
use crate::normalized::NormalizedQuery;
use crate::normalized::{
    ActivityRecord, AirdropRow, ArtifactRecord, CommentRecord, GovernanceProposalRecord,
    GovernanceVoteRecord, VersionRecord,
};
use crate::official_content::{
    OfficialBlogManifest, OfficialContentCache, OfficialContentConfig, OfficialContentResponse,
    OfficialContentService, OfficialContentWarmupReport, OfficialDocsManifest,
    OfficialForumManifest, blog_entries, blog_entry, docs_entries, docs_entry, forum_entries,
    forum_entry, official_cache_key,
};
use paperproof_sdk_rs::PaperProofQueryClient;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApiConfig {
    pub bind: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8787".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApiState {
    pub analytics: AnalyticsSummary,
    pub sqlite_path: Option<String>,
    pub postgres_url: Option<String>,
    pub official_content: OfficialContentConfig,
    pub official_cache: OfficialContentCache,
    pub explore_cache: ExploreContentCache,
}

impl Default for ApiState {
    fn default() -> Self {
        Self {
            analytics: AnalyticsSummary::default(),
            sqlite_path: None,
            postgres_url: None,
            official_content: OfficialContentConfig::default(),
            official_cache: Arc::new(RwLock::new(HashMap::new())),
            explore_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

pub type ExploreContentCache = Arc<RwLock<HashMap<String, ExploreCacheEntry>>>;

#[derive(Clone, Debug)]
pub struct ExploreCacheEntry {
    pub value: ExploreCacheValue,
}

#[derive(Clone, Debug)]
pub enum ExploreCacheValue {
    Summary(ExploreSummaryResponse),
    Items(ExploreItemsResponse),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PageParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub artifact_type: Option<u64>,
    pub sort: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub artifact_type: Option<u64>,
    pub owner: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ActivityParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub actor: Option<String>,
    pub series_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MetricsResponse {
    pub service: String,
    pub database_ready: bool,
    pub summary: AnalyticsSummary,
    pub ingest: IndexerMetricSnapshot,
}

#[derive(Clone, Debug, Serialize)]
pub struct ArtifactDetailResponse {
    pub artifact: Option<ArtifactRecord>,
    pub versions: Vec<VersionRecord>,
    pub comments: Vec<CommentRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreSummaryResponse {
    pub refreshed_at: String,
    pub per_type: u64,
    pub types: Vec<ExploreTypeSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreTypeSummary {
    pub artifact_type: u64,
    pub slug: String,
    pub total_indexed: u64,
    pub items: Vec<ExploreArtifactItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreItemsResponse {
    pub refreshed_at: String,
    pub artifact_type: Option<u64>,
    pub sort: String,
    pub limit: u64,
    pub offset: u64,
    pub has_more: bool,
    pub items: Vec<ExploreArtifactItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreLookupResponse {
    pub refreshed_at: String,
    pub artifact: Option<ExploreArtifactItem>,
    pub versions: Vec<VersionRecord>,
    pub comments: Vec<CommentRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreArtifactItem {
    pub series_id: String,
    pub artifact_code: String,
    pub artifact_type: u64,
    pub type_slug: String,
    pub title: String,
    pub summary: String,
    pub owner: String,
    pub authors: Vec<String>,
    pub status: String,
    pub published_at: String,
    pub updated_at: String,
    pub latest_version_id: String,
    pub latest_version_number: u64,
    pub comments_tree_id: String,
    pub likes_book_id: String,
    pub comment_count: Option<u64>,
    pub like_count: Option<u64>,
    pub content_hash: String,
    pub walrus_blob_id: String,
    pub content_type: String,
    pub license: String,
    pub field: String,
    pub keywords: Vec<String>,
    pub raw_artifact: serde_json::Value,
    pub raw_version: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: String,
}

#[derive(Debug)]
pub struct ApiError(paperproof_sdk_rs::PaperProofError);

type ApiResult<T> = Result<Json<T>, ApiError>;

impl From<paperproof_sdk_rs::PaperProofError> for ApiError {
    fn from(value: paperproof_sdk_rs::PaperProofError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiErrorResponse {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

pub async fn run_api_server(config: ApiConfig, state: ApiState) -> paperproof_sdk_rs::Result<()> {
    spawn_official_content_warmup(state.clone());
    spawn_explore_content_warmup(state.clone());
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/analytics/summary", get(analytics))
        .route("/v1/analytics/airdrop-snapshot-plan", get(snapshot_plan))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(prometheus_metrics))
        .route("/v1/explore/summary", get(explore_summary))
        .route("/v1/explore/artifacts", get(explore_artifacts))
        .route("/v1/explore/items", get(explore_items))
        .route("/v1/explore/search", get(explore_search_artifacts))
        .route("/v1/search/artifacts", get(search_artifacts))
        .route("/v1/artifacts/lookup", get(artifact_lookup))
        .route("/v1/artifacts/{series_id}", get(artifact_detail))
        .route("/v1/artifacts/{series_id}/versions", get(artifact_versions))
        .route("/v1/artifacts/{series_id}/comments", get(artifact_comments))
        .route("/v1/activity", get(activity_feed))
        .route("/v1/governance/proposals", get(governance_proposals))
        .route("/v1/my/{address}/artifacts", get(my_artifacts))
        .route("/v1/my/{address}/votes", get(my_votes))
        .route("/v1/airdrop/snapshot", get(airdrop_snapshot))
        .route("/v1/official/docs/{section}", get(official_doc_section))
        .route(
            "/v1/official/docs/{section}/{topic}",
            get(official_doc_topic),
        )
        .route("/v1/official/blog/{slug}", get(official_blog_post))
        .route("/v1/official/forum/{slug}", get(official_forum_topic))
        .with_state(state);
    let listener = TcpListener::bind(&config.bind).await.map_err(|err| {
        paperproof_sdk_rs::PaperProofError::network(&config.bind, err.to_string())
    })?;
    axum::serve(listener, app)
        .await
        .map_err(|err| paperproof_sdk_rs::PaperProofError::network(&config.bind, err.to_string()))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "paperproof-indexer-reference".to_string(),
    })
}

async fn analytics(State(state): State<ApiState>) -> Json<AnalyticsSummary> {
    Json(match query(&state).await {
        Ok(query) => query.summary().await.unwrap_or(state.analytics),
        Err(_) => state.analytics,
    })
}

async fn snapshot_plan() -> Json<AirdropSnapshotPlan> {
    Json(AirdropSnapshotPlan::default())
}

async fn metrics(State(state): State<ApiState>) -> Json<MetricsResponse> {
    let summary = match query(&state).await {
        Ok(query) => query
            .summary()
            .await
            .unwrap_or_else(|_| state.analytics.clone()),
        Err(_) => state.analytics.clone(),
    };
    let ingest = metric_snapshot(&state).await.unwrap_or_default();
    Json(MetricsResponse {
        service: "paperproof-indexer-reference".to_string(),
        database_ready: state.sqlite_path.is_some() || state.postgres_url.is_some(),
        summary,
        ingest,
    })
}

async fn prometheus_metrics(State(state): State<ApiState>) -> Response {
    let snapshot = metric_snapshot(&state).await.unwrap_or_default();
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        snapshot.to_prometheus(),
    )
        .into_response()
}

async fn explore_artifacts(
    State(state): State<ApiState>,
    Query(params): Query<PageParams>,
) -> ApiResult<Vec<ArtifactRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .recent_artifacts(
                params.artifact_type,
                params.limit.unwrap_or(50),
                params.offset.unwrap_or(0),
            )
            .await?,
    ))
}

async fn explore_items(
    State(state): State<ApiState>,
    Query(params): Query<PageParams>,
) -> ApiResult<ExploreItemsResponse> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).min(2_000);
    let sort = normalize_explore_sort(params.sort.as_deref());
    let cache_key = format!(
        "explore:list:{}:{sort}:{limit}:{offset}",
        params
            .artifact_type
            .map(|value| value.to_string())
            .unwrap_or_else(|| "all".to_string())
    );
    if let Some(ExploreCacheValue::Items(cached)) = cached_explore_value(&state, &cache_key).await {
        return Ok(Json(cached));
    }
    let rendered = build_explore_items(&state, params.artifact_type, &sort, limit, offset).await?;
    cache_explore_value(
        &state,
        cache_key,
        ExploreCacheValue::Items(rendered.clone()),
    )
    .await;
    Ok(Json(rendered))
}

async fn explore_summary(
    State(state): State<ApiState>,
    Query(params): Query<PageParams>,
) -> ApiResult<ExploreSummaryResponse> {
    let per_type = params.limit.unwrap_or(5).clamp(4, 20);
    let cache_key = format!("explore:summary:{per_type}");
    if let Some(ExploreCacheValue::Summary(cached)) = cached_explore_value(&state, &cache_key).await
    {
        return Ok(Json(cached));
    }
    let query = query(&state).await?;
    let mut types = Vec::new();
    for artifact_type in 1..=6 {
        let items = explore_items_from_records(
            &query,
            query
                .recent_artifacts(Some(artifact_type), per_type, 0)
                .await?,
        )
        .await?;
        let total_indexed = query
            .count_artifacts(Some(artifact_type))
            .await
            .unwrap_or(0);
        types.push(ExploreTypeSummary {
            artifact_type,
            slug: artifact_type_slug(artifact_type)
                .unwrap_or("unknown")
                .to_string(),
            total_indexed,
            items,
        });
    }
    let rendered = ExploreSummaryResponse {
        refreshed_at: now_rfc3339(),
        per_type,
        types,
    };
    cache_explore_value(
        &state,
        cache_key,
        ExploreCacheValue::Summary(rendered.clone()),
    )
    .await;
    Ok(Json(rendered))
}

async fn search_artifacts(
    State(state): State<ApiState>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Vec<ArtifactRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .search_artifacts(
                &params.q,
                params.artifact_type,
                params.owner.as_deref(),
                params.limit.unwrap_or(25),
                params.offset.unwrap_or(0),
            )
            .await?,
    ))
}

async fn explore_search_artifacts(
    State(state): State<ApiState>,
    Query(params): Query<SearchParams>,
) -> ApiResult<ExploreItemsResponse> {
    let limit = params.limit.unwrap_or(25).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).min(2_000);
    let query = query(&state).await?;
    let records = query
        .search_artifacts(
            &params.q,
            params.artifact_type,
            params.owner.as_deref(),
            limit + 1,
            offset,
        )
        .await?;
    let has_more = records.len() as u64 > limit;
    let items =
        explore_items_from_records(&query, records.into_iter().take(limit as usize).collect())
            .await?;
    Ok(Json(ExploreItemsResponse {
        refreshed_at: now_rfc3339(),
        artifact_type: params.artifact_type,
        sort: "search".to_string(),
        limit,
        offset,
        has_more,
        items,
    }))
}

async fn artifact_lookup(
    State(state): State<ApiState>,
    Query(params): Query<SearchParams>,
) -> ApiResult<ExploreLookupResponse> {
    let term = params.q.trim();
    let query = query(&state).await?;
    let artifact = query.lookup_artifact(term).await?;
    let Some(artifact) = artifact else {
        return Ok(Json(ExploreLookupResponse {
            refreshed_at: now_rfc3339(),
            artifact: None,
            versions: Vec::new(),
            comments: Vec::new(),
        }));
    };
    let versions = query.versions(&artifact.series_id).await?;
    let comments = query.comments(&artifact.series_id, 100, 0).await?;
    let item = explore_item_from_record(&query, artifact, versions.clone()).await?;
    Ok(Json(ExploreLookupResponse {
        refreshed_at: now_rfc3339(),
        artifact: Some(item),
        versions,
        comments,
    }))
}

async fn artifact_detail(
    State(state): State<ApiState>,
    Path(series_id): Path<String>,
    Query(params): Query<PageParams>,
) -> ApiResult<ArtifactDetailResponse> {
    let query = query(&state).await?;
    Ok(Json(ArtifactDetailResponse {
        artifact: query.artifact_detail(&series_id).await?,
        versions: query.versions(&series_id).await?,
        comments: query
            .comments(
                &series_id,
                params.limit.unwrap_or(25),
                params.offset.unwrap_or(0),
            )
            .await?,
    }))
}

async fn artifact_versions(
    State(state): State<ApiState>,
    Path(series_id): Path<String>,
) -> ApiResult<Vec<VersionRecord>> {
    Ok(Json(query(&state).await?.versions(&series_id).await?))
}

async fn artifact_comments(
    State(state): State<ApiState>,
    Path(series_id): Path<String>,
    Query(params): Query<PageParams>,
) -> ApiResult<Vec<CommentRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .comments(
                &series_id,
                params.limit.unwrap_or(25),
                params.offset.unwrap_or(0),
            )
            .await?,
    ))
}

async fn activity_feed(
    State(state): State<ApiState>,
    Query(params): Query<ActivityParams>,
) -> ApiResult<Vec<ActivityRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .activity(
                params.actor.as_deref(),
                params.series_id.as_deref(),
                params.limit.unwrap_or(50),
                params.offset.unwrap_or(0),
            )
            .await?,
    ))
}

async fn governance_proposals(
    State(state): State<ApiState>,
    Query(params): Query<PageParams>,
) -> ApiResult<Vec<GovernanceProposalRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .proposals(params.limit.unwrap_or(50), params.offset.unwrap_or(0))
            .await?,
    ))
}

async fn my_artifacts(
    State(state): State<ApiState>,
    Path(address): Path<String>,
    Query(params): Query<PageParams>,
) -> ApiResult<Vec<ArtifactRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .artifacts_by_owner(
                &address,
                params.limit.unwrap_or(10),
                params.offset.unwrap_or(0),
            )
            .await?,
    ))
}

async fn my_votes(
    State(state): State<ApiState>,
    Path(address): Path<String>,
    Query(params): Query<PageParams>,
) -> ApiResult<Vec<GovernanceVoteRecord>> {
    Ok(Json(
        query(&state)
            .await?
            .votes_for_address(
                &address,
                params.limit.unwrap_or(5),
                params.offset.unwrap_or(0),
            )
            .await?,
    ))
}

async fn airdrop_snapshot(State(state): State<ApiState>) -> ApiResult<Vec<AirdropRow>> {
    Ok(Json(query(&state).await?.airdrop_rows().await?))
}

async fn official_doc_section(
    State(state): State<ApiState>,
    Path(section): Path<String>,
) -> ApiResult<OfficialContentResponse> {
    official_docs_response(state, section, None).await
}

async fn official_doc_topic(
    State(state): State<ApiState>,
    Path((section, topic)): Path<(String, String)>,
) -> ApiResult<OfficialContentResponse> {
    official_docs_response(state, section, Some(topic)).await
}

async fn official_docs_response(
    state: ApiState,
    section: String,
    topic: Option<String>,
) -> ApiResult<OfficialContentResponse> {
    let slug = topic
        .as_ref()
        .map(|topic| format!("{section}/{topic}"))
        .unwrap_or_else(|| section.clone());
    if let Some(cached) = cached_official_content(&state, "docs", &slug).await {
        return Ok(Json(cached));
    }
    let service = OfficialContentService::new(state.official_content.clone());
    let manifest = service
        .load_manifest::<OfficialDocsManifest>("docs/manifest.json")
        .await?;
    let entry = docs_entry(manifest, &section, topic.as_deref()).ok_or_else(|| {
        paperproof_sdk_rs::PaperProofError::invalid_input("official docs slug", "entry not found")
    })?;
    let series_id = entry.series_id.clone().ok_or_else(|| {
        paperproof_sdk_rs::PaperProofError::invalid_input(
            "seriesId",
            "official docs entry has no seriesId",
        )
    })?;
    let versions = official_versions_or_empty(&state, &series_id).await;
    let rendered = service.render_entry("docs", &slug, entry, versions).await?;
    cache_official_content(&state, &rendered).await;
    Ok(Json(rendered))
}

async fn official_blog_post(
    State(state): State<ApiState>,
    Path(slug): Path<String>,
) -> ApiResult<OfficialContentResponse> {
    if let Some(cached) = cached_official_content(&state, "blog", &slug).await {
        return Ok(Json(cached));
    }
    let service = OfficialContentService::new(state.official_content.clone());
    let manifest = service
        .load_manifest::<OfficialBlogManifest>("blog/manifest.json")
        .await?;
    let entry = blog_entry(manifest, &slug).ok_or_else(|| {
        paperproof_sdk_rs::PaperProofError::invalid_input("official blog slug", "entry not found")
    })?;
    let series_id = entry.series_id.clone().ok_or_else(|| {
        paperproof_sdk_rs::PaperProofError::invalid_input(
            "seriesId",
            "official blog entry has no seriesId",
        )
    })?;
    let versions = official_versions_or_empty(&state, &series_id).await;
    let rendered = service.render_entry("blog", &slug, entry, versions).await?;
    cache_official_content(&state, &rendered).await;
    Ok(Json(rendered))
}

async fn official_forum_topic(
    State(state): State<ApiState>,
    Path(slug): Path<String>,
) -> ApiResult<OfficialContentResponse> {
    if let Some(cached) = cached_official_content(&state, "forum", &slug).await {
        return Ok(Json(cached));
    }
    let service = OfficialContentService::new(state.official_content.clone());
    let manifest = service
        .load_manifest::<OfficialForumManifest>("forum/manifest.json")
        .await?;
    let entry = forum_entry(manifest, &slug).ok_or_else(|| {
        paperproof_sdk_rs::PaperProofError::invalid_input("official forum slug", "entry not found")
    })?;
    let series_id = entry.series_id.clone().ok_or_else(|| {
        paperproof_sdk_rs::PaperProofError::invalid_input(
            "seriesId",
            "official forum entry has no seriesId",
        )
    })?;
    let versions = official_versions_or_empty(&state, &series_id).await;
    let rendered = service
        .render_entry("forum", &slug, entry, versions)
        .await?;
    cache_official_content(&state, &rendered).await;
    Ok(Json(rendered))
}

async fn cached_official_content(
    state: &ApiState,
    surface: &str,
    slug: &str,
) -> Option<OfficialContentResponse> {
    state
        .official_cache
        .read()
        .await
        .get(&official_cache_key(surface, slug))
        .cloned()
}

async fn cache_official_content(state: &ApiState, rendered: &OfficialContentResponse) {
    state.official_cache.write().await.insert(
        official_cache_key(&rendered.surface, &rendered.slug),
        rendered.clone(),
    );
}

pub async fn refresh_official_content_cache(
    state: ApiState,
) -> paperproof_sdk_rs::Result<OfficialContentWarmupReport> {
    warm_official_content_cache(state).await
}

fn spawn_official_content_warmup(state: ApiState) {
    tokio::spawn(async move {
        match warm_official_content_cache(state).await {
            Ok(report) => info!(
                attempted = report.attempted,
                cached = report.cached,
                failed = report.failed,
                "official content cache warmup completed"
            ),
            Err(error) => warn!(error = %error, "official content cache warmup failed"),
        }
    });
}

fn spawn_explore_content_warmup(state: ApiState) {
    tokio::spawn(async move {
        match refresh_explore_content_cache(state).await {
            Ok(count) => info!(entries = count, "explore content cache warmup completed"),
            Err(error) => warn!(error = %error, "explore content cache warmup failed"),
        }
    });
}

async fn warm_official_content_cache(
    state: ApiState,
) -> paperproof_sdk_rs::Result<OfficialContentWarmupReport> {
    let service = OfficialContentService::new(state.official_content.clone());
    let mut report = OfficialContentWarmupReport::default();
    let mut refreshed = HashMap::new();
    let docs = service
        .load_manifest::<OfficialDocsManifest>("docs/manifest.json")
        .await?;
    for (slug, entry) in docs_entries(docs) {
        warm_one_official_entry(
            &state,
            &service,
            "docs",
            &slug,
            entry,
            &mut report,
            &mut refreshed,
        )
        .await;
    }
    let blog = service
        .load_manifest::<OfficialBlogManifest>("blog/manifest.json")
        .await?;
    for (slug, entry) in blog_entries(blog) {
        warm_one_official_entry(
            &state,
            &service,
            "blog",
            &slug,
            entry,
            &mut report,
            &mut refreshed,
        )
        .await;
    }
    let forum = service
        .load_manifest::<OfficialForumManifest>("forum/manifest.json")
        .await?;
    for (slug, entry) in forum_entries(forum) {
        warm_one_official_entry(
            &state,
            &service,
            "forum",
            &slug,
            entry,
            &mut report,
            &mut refreshed,
        )
        .await;
    }
    replace_official_cache_entries(&state, refreshed).await;
    Ok(report)
}

async fn warm_one_official_entry(
    state: &ApiState,
    service: &OfficialContentService,
    surface: &str,
    slug: &str,
    entry: crate::official_content::OfficialEntry,
    report: &mut OfficialContentWarmupReport,
    refreshed: &mut HashMap<String, OfficialContentResponse>,
) {
    report.attempted += 1;
    let Some(series_id) = entry.series_id.clone() else {
        report.failed += 1;
        report
            .errors
            .push(format!("{surface}:{slug}: missing seriesId"));
        return;
    };
    let versions = official_versions_or_empty(state, &series_id).await;
    match service.render_entry(surface, slug, entry, versions).await {
        Ok(rendered) => {
            refreshed.insert(official_cache_key(surface, slug), rendered);
            report.cached += 1;
        }
        Err(error) => {
            report.failed += 1;
            report.errors.push(format!("{surface}:{slug}: {error}"));
        }
    }
}

async fn replace_official_cache_entries(
    state: &ApiState,
    refreshed: HashMap<String, OfficialContentResponse>,
) {
    let mut cache = state.official_cache.write().await;
    cache.retain(|key, _| {
        !(key.starts_with("docs:") || key.starts_with("blog:") || key.starts_with("forum:"))
    });
    cache.extend(refreshed);
}

async fn official_versions_or_empty(state: &ApiState, series_id: &str) -> Vec<VersionRecord> {
    match query(state).await {
        Ok(query) => query.versions(series_id).await.unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

async fn cached_explore_value(state: &ApiState, key: &str) -> Option<ExploreCacheValue> {
    state
        .explore_cache
        .read()
        .await
        .get(key)
        .map(|entry| entry.value.clone())
}

async fn cache_explore_value(state: &ApiState, key: String, value: ExploreCacheValue) {
    state
        .explore_cache
        .write()
        .await
        .insert(key, ExploreCacheEntry { value });
}

pub async fn refresh_explore_content_cache(state: ApiState) -> paperproof_sdk_rs::Result<usize> {
    let mut next = HashMap::new();
    let query = query(&state).await?;
    let per_type = 5;
    let mut types = Vec::new();
    for artifact_type in 1..=6 {
        let items = explore_items_from_records(
            &query,
            query
                .recent_artifacts(Some(artifact_type), per_type, 0)
                .await?,
        )
        .await?;
        let total_indexed = query
            .count_artifacts(Some(artifact_type))
            .await
            .unwrap_or(0);
        types.push(ExploreTypeSummary {
            artifact_type,
            slug: artifact_type_slug(artifact_type)
                .unwrap_or("unknown")
                .to_string(),
            total_indexed,
            items,
        });
    }
    next.insert(
        "explore:summary:5".to_string(),
        ExploreCacheEntry {
            value: ExploreCacheValue::Summary(ExploreSummaryResponse {
                refreshed_at: now_rfc3339(),
                per_type,
                types,
            }),
        },
    );
    for artifact_type in 1..=6 {
        let rendered = build_explore_items(&state, Some(artifact_type), "newest", 50, 0).await?;
        next.insert(
            format!("explore:list:{artifact_type}:newest:50:0"),
            ExploreCacheEntry {
                value: ExploreCacheValue::Items(rendered),
            },
        );
    }
    let count = next.len();
    *state.explore_cache.write().await = next;
    Ok(count)
}

async fn build_explore_items(
    state: &ApiState,
    artifact_type: Option<u64>,
    sort: &str,
    limit: u64,
    offset: u64,
) -> paperproof_sdk_rs::Result<ExploreItemsResponse> {
    let query = query(state).await?;
    let records = query
        .recent_artifacts(artifact_type, limit + 1, offset)
        .await?;
    let has_more = records.len() as u64 > limit && offset + limit < 2_000;
    let mut items =
        explore_items_from_records(&query, records.into_iter().take(limit as usize).collect())
            .await?;
    if sort == "discussed" {
        items.sort_by(|left, right| {
            right
                .comment_count
                .unwrap_or(0)
                .cmp(&left.comment_count.unwrap_or(0))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
    } else if sort == "updated" || sort == "liked" {
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    } else {
        items.sort_by(|left, right| right.published_at.cmp(&left.published_at));
    }
    Ok(ExploreItemsResponse {
        refreshed_at: now_rfc3339(),
        artifact_type,
        sort: sort.to_string(),
        limit,
        offset,
        has_more,
        items,
    })
}

async fn explore_items_from_records(
    query: &ApiQuery,
    records: Vec<ArtifactRecord>,
) -> paperproof_sdk_rs::Result<Vec<ExploreArtifactItem>> {
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        let versions = query.versions(&record.series_id).await.unwrap_or_default();
        items.push(explore_item_from_record(query, record, versions).await?);
    }
    Ok(items)
}

async fn explore_item_from_record(
    query: &ApiQuery,
    record: ArtifactRecord,
    versions: Vec<VersionRecord>,
) -> paperproof_sdk_rs::Result<ExploreArtifactItem> {
    let artifact_type = record.artifact_type.unwrap_or(0);
    let mut latest_version = choose_latest_version_for_explore(&record, &versions);
    if let Some(version) = latest_version.as_mut() {
        hydrate_explore_version(version).await;
    }
    let raw_version = latest_version
        .as_ref()
        .map(|version| version.raw_json.clone());
    let raw = latest_version
        .as_ref()
        .map(|version| version.raw_json.clone())
        .unwrap_or_else(|| record.raw_json.clone());
    let comment_count = query.count_comments(&record.series_id).await.ok();
    Ok(ExploreArtifactItem {
        series_id: record.series_id.clone(),
        artifact_code: record
            .artifact_code
            .clone()
            .unwrap_or_else(|| format!("Artifact {}", record.series_id)),
        artifact_type,
        type_slug: artifact_type_slug(artifact_type)
            .unwrap_or("generic-files")
            .to_string(),
        title: title_from_explore_raw(artifact_type, &raw, record.artifact_code.as_deref()),
        summary: summary_from_explore_raw(artifact_type, &raw),
        owner: record.owner.clone().unwrap_or_default(),
        authors: authors_from_explore_raw(artifact_type, &raw, record.owner.as_deref()),
        status: if record.status.unwrap_or(0) == 0 {
            "Active".to_string()
        } else {
            "Paused".to_string()
        },
        published_at: date_from_timestamp_string(record.published_at.as_deref()),
        updated_at: date_from_timestamp_string(record.updated_at.as_deref()),
        latest_version_id: record
            .latest_version_id
            .clone()
            .or_else(|| {
                latest_version
                    .as_ref()
                    .map(|version| version.version_id.clone())
            })
            .unwrap_or_default(),
        latest_version_number: latest_version
            .as_ref()
            .and_then(|version| version.version)
            .unwrap_or(1),
        comments_tree_id: record.comments_tree_id.clone().unwrap_or_default(),
        likes_book_id: record.likes_book_id.clone().unwrap_or_default(),
        comment_count,
        like_count: None,
        content_hash: latest_version
            .as_ref()
            .and_then(|version| version.content_hash.clone())
            .unwrap_or_else(|| "Not stored".to_string()),
        walrus_blob_id: latest_version
            .as_ref()
            .and_then(|version| version.walrus_blob_id.clone())
            .unwrap_or_else(|| "Not loaded".to_string()),
        content_type: latest_version
            .as_ref()
            .and_then(|version| version.content_type.clone())
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        license: string_field(&raw, "license").unwrap_or_else(|| "Not specified".to_string()),
        field: field_from_explore_raw(artifact_type, &raw),
        keywords: if artifact_type == 2 {
            string_list_field(&raw, "tags")
        } else {
            string_list_field(&raw, "keywords")
        },
        raw_artifact: record.raw_json,
        raw_version,
    })
}

async fn hydrate_explore_version(version: &mut VersionRecord) {
    if explore_raw_has_display_fields(&version.raw_json) {
        return;
    }
    let Ok(view) = PaperProofQueryClient::mainnet()
        .read
        .get_version_view(&version.version_id)
        .await
    else {
        return;
    };
    version.raw_json = view.raw_fields.clone();
    version.artifact_type = version.artifact_type.or(view.artifact_type.map(u64::from));
    version.version = version.version.or(view.version);
    version.content_hash = version.content_hash.clone().or(view.content_hash);
    version.walrus_blob_id = version
        .walrus_blob_id
        .clone()
        .or_else(|| json_string_path(&version.raw_json, &["header", "fields", "walrus_blob_id"]));
    version.content_type = version
        .content_type
        .clone()
        .or_else(|| json_string_path(&version.raw_json, &["header", "fields", "content_type"]));
}

fn explore_raw_has_display_fields(value: &serde_json::Value) -> bool {
    [
        "title",
        "abstract_text",
        "summary",
        "description",
        "changelog",
        "project_name",
    ]
    .iter()
    .any(|key| value.get(*key).is_some())
}

fn choose_latest_version_for_explore(
    record: &ArtifactRecord,
    versions: &[VersionRecord],
) -> Option<VersionRecord> {
    if let Some(latest_id) = &record.latest_version_id {
        if let Some(version) = versions
            .iter()
            .find(|version| &version.version_id == latest_id)
        {
            return Some(version.clone());
        }
    }
    versions
        .iter()
        .max_by_key(|version| version.version.unwrap_or(0))
        .cloned()
}

fn normalize_explore_sort(value: Option<&str>) -> String {
    match value {
        Some("updated") => "updated".to_string(),
        Some("discussed") => "discussed".to_string(),
        Some("liked") => "liked".to_string(),
        _ => "newest".to_string(),
    }
}

fn artifact_type_slug(value: u64) -> Option<&'static str> {
    match value {
        1 => Some("preprints"),
        2 => Some("blog-posts"),
        3 => Some("technical-reports"),
        4 => Some("datasets"),
        5 => Some("software-releases"),
        6 => Some("generic-files"),
        _ => None,
    }
}

fn title_from_explore_raw(
    artifact_type: u64,
    raw: &serde_json::Value,
    fallback: Option<&str>,
) -> String {
    if artifact_type == 5 {
        return string_field(raw, "project_name")
            .unwrap_or_else(|| fallback.unwrap_or("Artifact").to_string());
    }
    string_field(raw, "title").unwrap_or_else(|| fallback.unwrap_or("Artifact").to_string())
}

fn summary_from_explore_raw(artifact_type: u64, raw: &serde_json::Value) -> String {
    match artifact_type {
        1 | 3 => string_field(raw, "abstract_text")
            .unwrap_or_else(|| "No abstract stored on the latest readable version.".to_string()),
        2 => string_field(raw, "summary")
            .unwrap_or_else(|| "No summary stored on the latest readable version.".to_string()),
        5 => string_field(raw, "changelog")
            .unwrap_or_else(|| "No changelog stored on the latest readable version.".to_string()),
        _ => string_field(raw, "description")
            .unwrap_or_else(|| "No description stored on the latest readable version.".to_string()),
    }
}

fn authors_from_explore_raw(
    artifact_type: u64,
    raw: &serde_json::Value,
    owner: Option<&str>,
) -> Vec<String> {
    if artifact_type == 2 {
        if let Some(author) = string_field(raw, "author_name") {
            return vec![author];
        }
    }
    let authors = string_list_field(raw, "authors");
    if !authors.is_empty() {
        return authors;
    }
    owner
        .map(|value| vec![value.to_string()])
        .unwrap_or_default()
}

fn field_from_explore_raw(artifact_type: u64, raw: &serde_json::Value) -> String {
    match artifact_type {
        5 => string_field(raw, "repository_url").unwrap_or_else(|| "Software".to_string()),
        6 => string_field(raw, "filename").unwrap_or_else(|| "File".to_string()),
        4 => string_field(raw, "format").unwrap_or_else(|| "Dataset".to_string()),
        _ => string_field(raw, "field")
            .or_else(|| string_field(raw, "organization"))
            .unwrap_or_else(|| "Artifact".to_string()),
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(value_to_string)
        .filter(|v| !v.is_empty())
}

fn string_list_field(value: &serde_json::Value, key: &str) -> Vec<String> {
    let Some(item) = value.get(key) else {
        return Vec::new();
    };
    if let Some(values) = item.as_array() {
        return values.iter().filter_map(value_to_string).collect();
    }
    value_to_string(item)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn value_to_string(value: &serde_json::Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.trim().to_string());
    }
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(|value| value.trim().to_string())
}

fn json_string_path(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    value_to_string(cursor)
}

fn date_from_timestamp_string(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "On-chain".to_string();
    };
    if value.contains('T') {
        return value.chars().take(10).collect();
    }
    if let Ok(ms) = value.parse::<u64>() {
        if ms > 0 {
            let days = ms / 86_400_000;
            return format!("epoch+{days}d");
        }
    }
    value.chars().take(10).collect()
}

fn now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("unix:{seconds}")
}

enum ApiQuery {
    #[cfg(feature = "sqlite")]
    Sqlite(NormalizedQuery),
    #[cfg(feature = "postgres")]
    Postgres(crate::normalized::PostgresNormalizedQuery),
    Unavailable,
}

async fn query(state: &ApiState) -> paperproof_sdk_rs::Result<ApiQuery> {
    #[cfg(feature = "postgres")]
    if let Some(url) = &state.postgres_url {
        return Ok(ApiQuery::Postgres(
            crate::normalized::PostgresNormalizedQuery::connect(url).await?,
        ));
    }
    #[cfg(feature = "sqlite")]
    if let Some(path) = &state.sqlite_path {
        return Ok(ApiQuery::Sqlite(NormalizedQuery::sqlite(path)));
    }
    let _ = state;
    Ok(ApiQuery::Unavailable)
}

async fn metric_snapshot(state: &ApiState) -> paperproof_sdk_rs::Result<IndexerMetricSnapshot> {
    #[cfg(feature = "postgres")]
    if let Some(url) = &state.postgres_url {
        return crate::metrics::postgres_metric_snapshot(url).await;
    }
    #[cfg(feature = "sqlite")]
    if let Some(path) = &state.sqlite_path {
        return crate::metrics::sqlite_metric_snapshot(path);
    }
    let _ = state;
    Ok(IndexerMetricSnapshot::default())
}

impl ApiQuery {
    async fn summary(&self) -> paperproof_sdk_rs::Result<AnalyticsSummary> {
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.summary(),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.summary().await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn recent_artifacts(
        &self,
        artifact_type: Option<u64>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let _ = (artifact_type, limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.recent_artifacts(artifact_type, limit, offset),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.recent_artifacts(artifact_type, limit, offset).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn artifact_detail(
        &self,
        series_id: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        let _ = series_id;
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.artifact_detail(series_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.artifact_detail(series_id).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn lookup_artifact(
        &self,
        term: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        let _ = term;
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.lookup_artifact(term),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.lookup_artifact(term).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn count_artifacts(&self, artifact_type: Option<u64>) -> paperproof_sdk_rs::Result<u64> {
        let _ = artifact_type;
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.count_artifacts(artifact_type),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.count_artifacts(artifact_type).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn count_comments(&self, series_id: &str) -> paperproof_sdk_rs::Result<u64> {
        let _ = series_id;
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.count_comments(series_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.count_comments(series_id).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn versions(&self, series_id: &str) -> paperproof_sdk_rs::Result<Vec<VersionRecord>> {
        let _ = series_id;
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.versions(series_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.versions(series_id).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn comments(
        &self,
        series_id: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<CommentRecord>> {
        let _ = (series_id, limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.comments(series_id, limit, offset),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.comments(series_id, limit, offset).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn search_artifacts(
        &self,
        term: &str,
        artifact_type: Option<u64>,
        owner: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let _ = (term, artifact_type, owner, limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => {
                query.search_artifacts(term, artifact_type, owner, limit, offset)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => {
                query
                    .search_artifacts(term, artifact_type, owner, limit, offset)
                    .await
            }
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn activity(
        &self,
        actor: Option<&str>,
        series_id: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ActivityRecord>> {
        let _ = (actor, series_id, limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.activity(actor, series_id, limit, offset),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.activity(actor, series_id, limit, offset).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn proposals(
        &self,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceProposalRecord>> {
        let _ = (limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.proposals(limit, offset),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.proposals(limit, offset).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn artifacts_by_owner(
        &self,
        owner: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let _ = (owner, limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.artifacts_by_owner(owner, limit, offset),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.artifacts_by_owner(owner, limit, offset).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn votes_for_address(
        &self,
        address: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceVoteRecord>> {
        let _ = (address, limit, offset);
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.votes_for_address(address, limit, offset),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.votes_for_address(address, limit, offset).await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }

    async fn airdrop_rows(&self) -> paperproof_sdk_rs::Result<Vec<AirdropRow>> {
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite(query) => query.airdrop_rows(),
            #[cfg(feature = "postgres")]
            Self::Postgres(query) => query.airdrop_rows().await,
            Self::Unavailable => Err(api_backend_required()),
        }
    }
}

fn api_backend_required() -> paperproof_sdk_rs::PaperProofError {
    paperproof_sdk_rs::PaperProofError::invalid_input(
        "api backend",
        "serve requires --features sqlite or --features postgres and a configured backend",
    )
}
