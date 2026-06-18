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
}

impl Default for ApiState {
    fn default() -> Self {
        Self {
            analytics: AnalyticsSummary::default(),
            sqlite_path: None,
            postgres_url: None,
            official_content: OfficialContentConfig::default(),
            official_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
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
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/analytics/summary", get(analytics))
        .route("/v1/analytics/airdrop-snapshot-plan", get(snapshot_plan))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(prometheus_metrics))
        .route("/v1/explore/artifacts", get(explore_artifacts))
        .route("/v1/search/artifacts", get(search_artifacts))
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
