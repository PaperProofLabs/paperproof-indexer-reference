// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexerMetricSnapshot {
    pub processed_events: u64,
    pub rejected_events: u64,
    pub duplicate_events_skipped: u64,
    pub checkpoint_lag: u64,
    pub db_write_latency_ms: u64,
    pub retry_count: u64,
    pub batches_written: u64,
    pub checkpoints_scanned: u64,
}

impl IndexerMetricSnapshot {
    pub fn to_prometheus(&self) -> String {
        let mut out = String::new();
        push_metric(
            &mut out,
            "paperproof_indexer_processed_events_total",
            "Accepted PaperProof events processed by the reference indexer.",
            self.processed_events,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_rejected_events_total",
            "Rejected PaperProof events observed by the reference indexer.",
            self.rejected_events,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_duplicate_events_skipped_total",
            "Duplicate event writes skipped by sinks.",
            self.duplicate_events_skipped,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_checkpoint_lag",
            "Estimated checkpoint lag for checkpoint ingestion or persisted cursors.",
            self.checkpoint_lag,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_db_write_latency_ms_total",
            "Cumulative database write latency in milliseconds.",
            self.db_write_latency_ms,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_retry_count_total",
            "Retry count recorded by ingestion workers.",
            self.retry_count,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_batches_written_total",
            "Event batches written by sinks.",
            self.batches_written,
        );
        push_metric(
            &mut out,
            "paperproof_indexer_checkpoints_scanned_total",
            "Checkpoints scanned by checkpoint ingestion.",
            self.checkpoints_scanned,
        );
        out
    }
}

#[cfg(feature = "sqlite")]
pub fn sqlite_record_ingest_metrics(
    db_path: &str,
    report: &crate::pipeline::BackfillReport,
) -> paperproof_sdk_rs::Result<()> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open metrics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure metrics schema"))?;
    set_metric(&conn, "processed_events", report.accepted_events)?;
    set_metric(&conn, "rejected_events", report.rejected_events)?;
    set_metric(
        &conn,
        "duplicate_events_skipped",
        u64::try_from(report.duplicate_skipped).unwrap_or(u64::MAX),
    )?;
    set_metric(&conn, "batches_written", report.pages_scanned)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
pub fn sqlite_record_replay_metrics(
    db_path: &str,
    report: &crate::pipeline::ReplayReport,
) -> paperproof_sdk_rs::Result<()> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open metrics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure metrics schema"))?;
    set_metric(&conn, "processed_events", report.domain_events_applied)?;
    set_metric(&conn, "batches_written", 1)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
pub fn sqlite_record_db_write_latency(
    db_path: &str,
    latency_ms: u64,
) -> paperproof_sdk_rs::Result<()> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open metrics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure metrics schema"))?;
    set_metric(&conn, "db_write_latency_ms", latency_ms)
}

#[cfg(feature = "sqlite")]
pub fn sqlite_record_checkpoint_metrics(
    db_path: &str,
    metrics: &paperproof_sdk_rs::IndexerMetrics,
) -> paperproof_sdk_rs::Result<()> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open metrics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure metrics schema"))?;
    set_metric(&conn, "processed_events", metrics.processed_events)?;
    set_metric(&conn, "rejected_events", metrics.rejected_events)?;
    set_metric(
        &conn,
        "duplicate_events_skipped",
        metrics.duplicate_events_skipped,
    )?;
    set_metric(
        &conn,
        "checkpoint_lag",
        metrics.checkpoint_lag.unwrap_or_default(),
    )?;
    set_metric(&conn, "db_write_latency_ms", metrics.db_write_latency_ms)?;
    set_metric(&conn, "retry_count", metrics.retry_count)?;
    set_metric(&conn, "batches_written", metrics.batches_written)?;
    set_metric(&conn, "checkpoints_scanned", metrics.checkpoints_scanned)?;
    Ok(())
}

#[cfg(not(feature = "sqlite"))]
pub fn sqlite_record_db_write_latency(
    _db_path: &str,
    _latency_ms: u64,
) -> paperproof_sdk_rs::Result<()> {
    Ok(())
}

#[cfg(feature = "sqlite")]
pub fn sqlite_metric_snapshot(db_path: &str) -> paperproof_sdk_rs::Result<IndexerMetricSnapshot> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open metrics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure metrics schema"))?;
    Ok(IndexerMetricSnapshot {
        processed_events: metric(&conn, "processed_events")?,
        rejected_events: metric(&conn, "rejected_events")?,
        duplicate_events_skipped: metric(&conn, "duplicate_events_skipped")?,
        checkpoint_lag: metric(&conn, "checkpoint_lag")?,
        db_write_latency_ms: metric(&conn, "db_write_latency_ms")?,
        retry_count: metric(&conn, "retry_count")?,
        batches_written: metric(&conn, "batches_written")?,
        checkpoints_scanned: metric(&conn, "checkpoints_scanned")?,
    })
}

#[cfg(feature = "postgres")]
pub async fn postgres_record_checkpoint_metrics(
    connection_string: &str,
    metrics: &paperproof_sdk_rs::IndexerMetrics,
) -> paperproof_sdk_rs::Result<()> {
    let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
        .await
        .map_err(postgres_err("connect postgres metrics"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("paperproof postgres metrics connection closed: {error}");
        }
    });
    client
        .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
        .await
        .map_err(postgres_err("ensure postgres metrics schema"))?;
    set_pg_metric(&client, "processed_events", metrics.processed_events).await?;
    set_pg_metric(&client, "rejected_events", metrics.rejected_events).await?;
    set_pg_metric(
        &client,
        "duplicate_events_skipped",
        metrics.duplicate_events_skipped,
    )
    .await?;
    set_pg_metric(
        &client,
        "checkpoint_lag",
        metrics.checkpoint_lag.unwrap_or_default(),
    )
    .await?;
    set_pg_metric(&client, "db_write_latency_ms", metrics.db_write_latency_ms).await?;
    set_pg_metric(&client, "retry_count", metrics.retry_count).await?;
    set_pg_metric(&client, "batches_written", metrics.batches_written).await?;
    set_pg_metric(&client, "checkpoints_scanned", metrics.checkpoints_scanned).await?;
    Ok(())
}

#[cfg(feature = "postgres")]
pub async fn postgres_metric_snapshot(
    connection_string: &str,
) -> paperproof_sdk_rs::Result<IndexerMetricSnapshot> {
    let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
        .await
        .map_err(postgres_err("connect postgres metrics"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("paperproof postgres metrics connection closed: {error}");
        }
    });
    client
        .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
        .await
        .map_err(postgres_err("ensure postgres metrics schema"))?;
    Ok(IndexerMetricSnapshot {
        processed_events: pg_metric(&client, "processed_events").await?,
        rejected_events: pg_metric(&client, "rejected_events").await?,
        duplicate_events_skipped: pg_metric(&client, "duplicate_events_skipped").await?,
        checkpoint_lag: pg_metric(&client, "checkpoint_lag").await?,
        db_write_latency_ms: pg_metric(&client, "db_write_latency_ms").await?,
        retry_count: pg_metric(&client, "retry_count").await?,
        batches_written: pg_metric(&client, "batches_written").await?,
        checkpoints_scanned: pg_metric(&client, "checkpoints_scanned").await?,
    })
}

#[cfg(not(feature = "sqlite"))]
pub fn sqlite_metric_snapshot(_db_path: &str) -> paperproof_sdk_rs::Result<IndexerMetricSnapshot> {
    Err(paperproof_sdk_rs::PaperProofError::invalid_input(
        "sqlite",
        "metrics require --features sqlite",
    ))
}

#[cfg(feature = "sqlite")]
fn set_metric(
    conn: &rusqlite::Connection,
    name: &str,
    increment: u64,
) -> paperproof_sdk_rs::Result<()> {
    let value = u64_to_i64(increment)?;
    conn.execute(
        "insert into paperproof_indexer_metrics (name, value, updated_at)
         values (?1, ?2, current_timestamp)
         on conflict(name) do update set
            value = value + excluded.value,
            updated_at = current_timestamp",
        rusqlite::params![name, value],
    )
    .map_err(sqlite_err("upsert metric"))?;
    conn.execute(
        "insert into paperproof_indexer_metric_samples (name, value) values (?1, ?2)",
        rusqlite::params![name, value],
    )
    .map_err(sqlite_err("insert metric sample"))?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn metric(conn: &rusqlite::Connection, name: &str) -> paperproof_sdk_rs::Result<u64> {
    let value = conn
        .query_row(
            "select value from paperproof_indexer_metrics where name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    i64_to_u64(value)
}

#[cfg(feature = "postgres")]
async fn set_pg_metric(
    client: &tokio_postgres::Client,
    name: &str,
    increment: u64,
) -> paperproof_sdk_rs::Result<()> {
    let value = u64_to_i64(increment)?;
    client
        .execute(
            "insert into paperproof_indexer_metrics (name, value, updated_at)
             values ($1, $2, now())
             on conflict(name) do update set
                value = paperproof_indexer_metrics.value + excluded.value,
                updated_at = now()",
            &[&name, &value],
        )
        .await
        .map_err(postgres_err("postgres upsert metric"))?;
    client
        .execute(
            "insert into paperproof_indexer_metric_samples (name, value) values ($1, $2)",
            &[&name, &value],
        )
        .await
        .map_err(postgres_err("postgres insert metric sample"))?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn pg_metric(client: &tokio_postgres::Client, name: &str) -> paperproof_sdk_rs::Result<u64> {
    let value = client
        .query_opt(
            "select value from paperproof_indexer_metrics where name = $1",
            &[&name],
        )
        .await
        .map_err(postgres_err("postgres read metric"))?
        .map(|row| row.get::<_, i64>(0))
        .unwrap_or(0);
    i64_to_u64(value)
}

fn push_metric(out: &mut String, name: &str, help: &str, value: u64) {
    out.push_str("# HELP ");
    out.push_str(name);
    out.push(' ');
    out.push_str(help);
    out.push('\n');
    out.push_str("# TYPE ");
    out.push_str(name);
    out.push_str(" counter\n");
    out.push_str(name);
    out.push(' ');
    out.push_str(&value.to_string());
    out.push('\n');
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn u64_to_i64(value: u64) -> paperproof_sdk_rs::Result<i64> {
    i64::try_from(value)
        .map_err(|_| paperproof_sdk_rs::PaperProofError::invalid_input("u64", "value exceeds i64"))
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn i64_to_u64(value: i64) -> paperproof_sdk_rs::Result<u64> {
    u64::try_from(value).map_err(|_| {
        paperproof_sdk_rs::PaperProofError::invalid_input("metric", "negative metric value")
    })
}

#[cfg(feature = "postgres")]
fn postgres_err(
    context: &'static str,
) -> impl Fn(tokio_postgres::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| paperproof_sdk_rs::PaperProofError::network(context, err.to_string())
}

#[cfg(feature = "sqlite")]
fn sqlite_err(
    context: &'static str,
) -> impl Fn(rusqlite::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| paperproof_sdk_rs::PaperProofError::network(context, err.to_string())
}
