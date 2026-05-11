// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use async_trait::async_trait;
use paperproof_sdk_rs::{
    EventId, IndexerCursorStore, JsonlEventSink, MemoryIndexerCursorStore, PaperProofError,
    PaperProofEventSink, StoredIndexerCursor, StreamId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContentRef {
    pub source_event_key: String,
    pub artifact_id: Option<String>,
    pub version_id: Option<String>,
    pub blob_id: String,
    pub expected_sha256_hex: Option<String>,
    pub content_type: Option<String>,
}

#[async_trait]
pub trait ContentRefStore: Send + Sync {
    async fn list_pending_refs(&self, limit: usize) -> paperproof_sdk_rs::Result<Vec<ContentRef>>;

    async fn mark_enriched(
        &self,
        source_event_key: &str,
        status: &str,
        details_json: serde_json::Value,
    ) -> paperproof_sdk_rs::Result<()>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryContentRefStore {
    refs: Arc<Mutex<Vec<ContentRef>>>,
}

impl InMemoryContentRefStore {
    pub fn new(refs: Vec<ContentRef>) -> Self {
        Self {
            refs: Arc::new(Mutex::new(refs)),
        }
    }
}

#[async_trait]
impl ContentRefStore for InMemoryContentRefStore {
    async fn list_pending_refs(&self, limit: usize) -> paperproof_sdk_rs::Result<Vec<ContentRef>> {
        Ok(self
            .refs
            .lock()
            .expect("content ref store poisoned")
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn mark_enriched(
        &self,
        _source_event_key: &str,
        _status: &str,
        _details_json: serde_json::Value,
    ) -> paperproof_sdk_rs::Result<()> {
        Ok(())
    }
}

pub struct ReferenceEventSink {
    inner: Box<dyn PaperProofEventSink>,
    normalized_sqlite_path: Option<String>,
    normalized_postgres_url: Option<String>,
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct SqliteContentRefStore {
    path: String,
}

#[cfg(feature = "sqlite")]
impl SqliteContentRefStore {
    pub fn new(path: impl Into<String>) -> paperproof_sdk_rs::Result<Self> {
        let path = path.into();
        let conn =
            rusqlite::Connection::open(&path).map_err(sqlite_error("sqlite open content store"))?;
        conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
            .map_err(sqlite_error("sqlite ensure content schema"))?;
        Ok(Self { path })
    }

    fn connection(&self) -> paperproof_sdk_rs::Result<rusqlite::Connection> {
        rusqlite::Connection::open(&self.path).map_err(sqlite_error("sqlite open content store"))
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl ContentRefStore for SqliteContentRefStore {
    async fn list_pending_refs(&self, limit: usize) -> paperproof_sdk_rs::Result<Vec<ContentRef>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select source_event_key, artifact_id, version_id, blob_id, expected_sha256_hex, content_type from paperproof_content_refs where status in ('pending', 'fetch_failed') order by updated_at asc limit ?1")
            .map_err(sqlite_error("sqlite prepare pending refs"))?;
        stmt.query_map([usize_to_i64(limit)?], |row| {
            Ok(ContentRef {
                source_event_key: row.get(0)?,
                artifact_id: row.get(1)?,
                version_id: row.get(2)?,
                blob_id: row.get(3)?,
                expected_sha256_hex: row.get(4)?,
                content_type: row.get(5)?,
            })
        })
        .map_err(sqlite_error("sqlite query pending refs"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_error("sqlite read pending refs"))
    }

    async fn mark_enriched(
        &self,
        source_event_key: &str,
        status: &str,
        details_json: serde_json::Value,
    ) -> paperproof_sdk_rs::Result<()> {
        let conn = self.connection()?;
        let blob_id = details_json
            .get("blob_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        conn.execute(
            "update paperproof_content_refs set status = ?1, details_json = ?2, updated_at = current_timestamp where source_event_key = ?3",
            rusqlite::params![status, serde_json::to_string(&details_json)?, source_event_key],
        )
        .map_err(sqlite_error("sqlite update content ref"))?;
        conn.execute(
            "insert into paperproof_content_cache (
                blob_id, sha256_hex, byte_len, content_type, preview_utf8, status, error, updated_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
            on conflict(blob_id) do update set
                sha256_hex = excluded.sha256_hex,
                byte_len = excluded.byte_len,
                content_type = excluded.content_type,
                preview_utf8 = excluded.preview_utf8,
                status = excluded.status,
                error = excluded.error,
                updated_at = current_timestamp",
            rusqlite::params![
                blob_id,
                details_json
                    .get("sha256_hex")
                    .and_then(serde_json::Value::as_str),
                details_json
                    .get("byte_len")
                    .and_then(serde_json::Value::as_u64)
                    .map(u64_to_i64)
                    .transpose()?,
                details_json
                    .get("content_type")
                    .and_then(serde_json::Value::as_str),
                details_json
                    .get("preview_utf8")
                    .and_then(serde_json::Value::as_str),
                status,
                details_json
                    .get("error")
                    .and_then(serde_json::Value::as_str),
            ],
        )
        .map_err(sqlite_error("sqlite upsert content cache"))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FileCursorStore {
    path: PathBuf,
    inner: MemoryIndexerCursorStore,
}

impl FileCursorStore {
    pub fn new(path: impl Into<PathBuf>) -> paperproof_sdk_rs::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|err| {
                PaperProofError::network(parent.display().to_string(), err.to_string())
            })?;
        }
        Ok(Self {
            path,
            inner: MemoryIndexerCursorStore::default(),
        })
    }

    fn read_state(&self) -> paperproof_sdk_rs::Result<FileCursorState> {
        if !self.path.exists() {
            return Ok(FileCursorState::default());
        }
        let text = std::fs::read_to_string(&self.path).map_err(|err| {
            PaperProofError::network(self.path.display().to_string(), err.to_string())
        })?;
        serde_json::from_str(&text).map_err(Into::into)
    }

    fn write_state(&self, state: &FileCursorState) -> paperproof_sdk_rs::Result<()> {
        let text = serde_json::to_string_pretty(state)?;
        std::fs::write(&self.path, text).map_err(|err| {
            PaperProofError::network(self.path.display().to_string(), err.to_string())
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct FileCursorState {
    cursors: std::collections::BTreeMap<String, StoredIndexerCursor>,
    processed_event_keys: BTreeSet<String>,
}

#[async_trait]
impl IndexerCursorStore for FileCursorStore {
    async fn load_cursor(
        &self,
        stream: &StreamId,
    ) -> paperproof_sdk_rs::Result<Option<StoredIndexerCursor>> {
        Ok(self.read_state()?.cursors.get(&stream.0).cloned())
    }

    async fn save_cursor(
        &self,
        stream: &StreamId,
        cursor: StoredIndexerCursor,
    ) -> paperproof_sdk_rs::Result<()> {
        self.inner.save_cursor(stream, cursor.clone()).await?;
        let mut state = self.read_state()?;
        state.cursors.insert(stream.0.clone(), cursor);
        self.write_state(&state)
    }

    async fn mark_processed(&self, event_id: &EventId) -> paperproof_sdk_rs::Result<bool> {
        self.inner.mark_processed(event_id).await?;
        let mut state = self.read_state()?;
        let inserted = state.processed_event_keys.insert(event_id.key());
        self.write_state(&state)?;
        Ok(inserted)
    }
}

impl ReferenceEventSink {
    pub fn new(inner: Box<dyn PaperProofEventSink>) -> Self {
        Self {
            inner,
            normalized_sqlite_path: None,
            normalized_postgres_url: None,
        }
    }

    pub fn with_normalized_sqlite(inner: Box<dyn PaperProofEventSink>, path: String) -> Self {
        Self {
            inner,
            normalized_sqlite_path: Some(path),
            normalized_postgres_url: None,
        }
    }

    pub fn with_normalized_postgres(
        inner: Box<dyn PaperProofEventSink>,
        connection_string: String,
    ) -> Self {
        Self {
            inner,
            normalized_sqlite_path: None,
            normalized_postgres_url: Some(connection_string),
        }
    }
}

#[async_trait]
impl PaperProofEventSink for ReferenceEventSink {
    async fn write_batch(
        &self,
        batch: &paperproof_sdk_rs::IndexerEventBatch,
    ) -> paperproof_sdk_rs::Result<paperproof_sdk_rs::SinkWriteSummary> {
        let started = Instant::now();
        let summary = self.inner.write_batch(batch).await?;
        if let Some(path) = &self.normalized_sqlite_path {
            crate::normalized::apply_normalized_batch_sqlite(path, batch)?;
            crate::metrics::sqlite_record_db_write_latency(
                path,
                u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            )?;
        }
        if let Some(connection_string) = &self.normalized_postgres_url {
            crate::normalized::apply_normalized_batch_postgres(connection_string, batch).await?;
        }
        Ok(summary)
    }
}

pub async fn build_event_sink(
    sink: &str,
    output_dir: &str,
    prefix: &str,
) -> paperproof_sdk_rs::Result<Box<dyn PaperProofEventSink>> {
    match sink {
        "jsonl" => Ok(Box::new(JsonlEventSink::new(
            format!("{output_dir}/{prefix}-accepted.jsonl"),
            format!("{output_dir}/{prefix}-rejected.jsonl"),
        ))),
        "sqlite" => sqlite_sink(output_dir),
        "postgres" => postgres_sink().await,
        other => Err(PaperProofError::invalid_input(
            "sink",
            format!("unsupported sink `{other}`; expected jsonl, sqlite, or postgres"),
        )),
    }
}

pub async fn build_cursor_store(
    sink: &str,
    output_dir: &str,
) -> paperproof_sdk_rs::Result<Box<dyn IndexerCursorStore>> {
    match sink {
        "jsonl" => Ok(Box::new(FileCursorStore::new(cursor_path(output_dir))?)),
        "sqlite" => sqlite_cursor_store(output_dir),
        "postgres" => postgres_cursor_store().await,
        other => Err(PaperProofError::invalid_input(
            "sink",
            format!("unsupported sink `{other}`; expected jsonl, sqlite, or postgres"),
        )),
    }
}

fn cursor_path(output_dir: &str) -> PathBuf {
    Path::new(output_dir).join("cursor.json")
}

#[cfg(feature = "sqlite")]
fn sqlite_sink(output_dir: &str) -> paperproof_sdk_rs::Result<Box<dyn PaperProofEventSink>> {
    let path = sqlite_path(output_dir);
    let inner = Box::new(paperproof_sdk_rs::SqliteEventSink::new(path.clone())?);
    Ok(Box::new(ReferenceEventSink::with_normalized_sqlite(
        inner, path,
    )))
}

#[cfg(not(feature = "sqlite"))]
fn sqlite_sink(_output_dir: &str) -> paperproof_sdk_rs::Result<Box<dyn PaperProofEventSink>> {
    Err(PaperProofError::invalid_input(
        "sink",
        "sqlite sink requires `--features sqlite`",
    ))
}

#[cfg(feature = "sqlite")]
fn sqlite_cursor_store(output_dir: &str) -> paperproof_sdk_rs::Result<Box<dyn IndexerCursorStore>> {
    Ok(Box::new(paperproof_sdk_rs::SqliteCursorStore::new(
        sqlite_path(output_dir),
    )?))
}

#[cfg(not(feature = "sqlite"))]
fn sqlite_cursor_store(
    _output_dir: &str,
) -> paperproof_sdk_rs::Result<Box<dyn IndexerCursorStore>> {
    Err(PaperProofError::invalid_input(
        "sink",
        "sqlite cursor store requires `--features sqlite`",
    ))
}

#[cfg(feature = "sqlite")]
fn sqlite_path(output_dir: &str) -> String {
    std::env::var("PAPERPROOF_INDEXER_SQLITE_PATH")
        .unwrap_or_else(|_| format!("{output_dir}/paperproof-indexer-reference.sqlite"))
}

#[cfg(feature = "postgres")]
async fn postgres_sink() -> paperproof_sdk_rs::Result<Box<dyn PaperProofEventSink>> {
    let url = postgres_url()?;
    let inner = Box::new(paperproof_sdk_rs::PostgresEventSink::connect(&url).await?);
    Ok(Box::new(ReferenceEventSink::with_normalized_postgres(
        inner, url,
    )))
}

#[cfg(not(feature = "postgres"))]
async fn postgres_sink() -> paperproof_sdk_rs::Result<Box<dyn PaperProofEventSink>> {
    Err(PaperProofError::invalid_input(
        "sink",
        "postgres sink requires `--features postgres`",
    ))
}

#[cfg(feature = "postgres")]
async fn postgres_cursor_store() -> paperproof_sdk_rs::Result<Box<dyn IndexerCursorStore>> {
    Ok(Box::new(
        paperproof_sdk_rs::PostgresCursorStore::connect(&postgres_url()?).await?,
    ))
}

#[cfg(not(feature = "postgres"))]
async fn postgres_cursor_store() -> paperproof_sdk_rs::Result<Box<dyn IndexerCursorStore>> {
    Err(PaperProofError::invalid_input(
        "sink",
        "postgres cursor store requires `--features postgres`",
    ))
}

#[cfg(feature = "postgres")]
fn postgres_url() -> paperproof_sdk_rs::Result<String> {
    std::env::var("PAPERPROOF_INDEXER_POSTGRES_URL").map_err(|_| {
        PaperProofError::invalid_input(
            "PAPERPROOF_INDEXER_POSTGRES_URL",
            "set PAPERPROOF_INDEXER_POSTGRES_URL when sink=postgres",
        )
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_error(
    context: &'static str,
) -> impl Fn(rusqlite::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| PaperProofError::network(context, err.to_string())
}

#[cfg(feature = "sqlite")]
fn usize_to_i64(value: usize) -> paperproof_sdk_rs::Result<i64> {
    i64::try_from(value).map_err(|_| PaperProofError::invalid_input("usize", "value exceeds i64"))
}

#[cfg(feature = "sqlite")]
fn u64_to_i64(value: u64) -> paperproof_sdk_rs::Result<i64> {
    i64::try_from(value).map_err(|_| PaperProofError::invalid_input("u64", "value exceeds i64"))
}
