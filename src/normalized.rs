// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(any(feature = "sqlite", feature = "postgres"))]
use paperproof_sdk_rs::{IndexedPaperProofEvent, events::PaperProofEventKind};
use paperproof_sdk_rs::{IndexerEventBatch, PaperProofError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtifactRecord {
    pub series_id: String,
    pub artifact_code: Option<String>,
    pub artifact_type: Option<u64>,
    pub owner: Option<String>,
    pub latest_version_id: Option<String>,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub title: Option<String>,
    pub status: Option<u64>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VersionRecord {
    pub version_id: String,
    pub series_id: String,
    pub artifact_type: Option<u64>,
    pub version: Option<u64>,
    pub content_hash: Option<String>,
    pub walrus_blob_id: Option<String>,
    pub content_type: Option<String>,
    pub created_at: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CommentRecord {
    pub tree_id: String,
    pub comment_id: u64,
    pub parent_comment_id: Option<u64>,
    pub series_id: Option<String>,
    pub author: Option<String>,
    pub content_mode: Option<u64>,
    pub status: Option<u64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GovernanceProposalRecord {
    pub proposal_id: u64,
    pub proposal_object_id: Option<String>,
    pub proposer: Option<String>,
    pub title: Option<String>,
    pub action_type: Option<u64>,
    pub proposal_type: Option<u64>,
    pub status: Option<u64>,
    pub yes_votes: Option<String>,
    pub no_votes: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GovernanceVoteRecord {
    pub proposal_id: u64,
    pub voter: String,
    pub side: Option<u64>,
    pub voting_power: Option<String>,
    pub claimed: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActivityRecord {
    pub event_key: String,
    pub kind: String,
    pub actor: Option<String>,
    pub series_id: Option<String>,
    pub proposal_id: Option<u64>,
    pub tree_id: Option<String>,
    pub created_at: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AirdropRow {
    pub address: String,
    pub published_artifacts: u64,
    pub versions_added: u64,
    pub comments: u64,
    pub votes: u64,
    pub likes: u64,
    pub score: u64,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RebuildReport {
    pub source: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub normalized_tables_cleared: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AirdropFormat {
    Json,
    Csv,
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
pub struct NormalizedQuery {
    db_path: String,
}

#[cfg(not(feature = "sqlite"))]
#[derive(Clone, Debug)]
pub struct NormalizedQuery;

#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresNormalizedQuery {
    client: std::sync::Arc<tokio::sync::Mutex<tokio_postgres::Client>>,
}

#[cfg(feature = "sqlite")]
impl NormalizedQuery {
    pub fn sqlite(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    pub fn summary(&self) -> paperproof_sdk_rs::Result<crate::analytics::AnalyticsSummary> {
        let conn = self.connection()?;
        Ok(crate::analytics::AnalyticsSummary {
            total_artifacts: count(&conn, "domain_artifacts")?,
            total_versions: count(&conn, "domain_versions")?,
            total_comments: count(&conn, "domain_comments")?,
            total_likes: scalar_i64(
                &conn,
                "select coalesce(sum(likes), 0) from domain_airdrop_scores",
            )? as u64,
            total_proposals: count(&conn, "domain_governance_proposals")?,
            total_votes: count(&conn, "domain_votes")?,
            last_checkpoint: scalar_i64(
                &conn,
                "select coalesce(max(checkpoint), 0) from paperproof_events",
            )
            .ok()
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0),
            content_refs_pending: scalar_i64(
                &conn,
                "select count(*) from paperproof_content_refs where status = 'pending'",
            )? as u64,
            content_cache_verified: scalar_i64(
                &conn,
                "select count(*) from paperproof_content_cache where status = 'verified'",
            )? as u64,
            top_contributors: top_contributors(&conn, 10)?,
            artifact_types: artifact_type_summary(&conn)?,
        })
    }

    pub fn recent_artifacts(
        &self,
        artifact_type: Option<u64>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let conn = self.connection()?;
        let sql = if artifact_type.is_some() {
            "select series_id, artifact_code, artifact_type, owner, latest_version_id, comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json from domain_artifacts where artifact_type = ?1 order by coalesce(updated_at, published_at) desc limit ?2 offset ?3"
        } else {
            "select series_id, artifact_code, artifact_type, owner, latest_version_id, comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json from domain_artifacts order by coalesce(updated_at, published_at) desc limit ?1 offset ?2"
        };
        let mut stmt = conn.prepare(sql).map_err(sqlite_err("prepare artifacts"))?;
        let rows = if let Some(kind) = artifact_type {
            stmt.query_map(
                rusqlite::params![u64_to_i64(kind)?, u64_to_i64(limit)?, u64_to_i64(offset)?],
                artifact_from_row,
            )
            .map_err(sqlite_err("query artifacts"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_err("read artifacts"))?
        } else {
            stmt.query_map(
                rusqlite::params![u64_to_i64(limit)?, u64_to_i64(offset)?],
                artifact_from_row,
            )
            .map_err(sqlite_err("query artifacts"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_err("read artifacts"))?
        };
        Ok(rows)
    }

    pub fn artifact_detail(
        &self,
        series_id: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select series_id, artifact_code, artifact_type, owner, latest_version_id, comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json from domain_artifacts where series_id = ?1")
            .map_err(sqlite_err("prepare artifact detail"))?;
        let mut rows = stmt
            .query_map([series_id], artifact_from_row)
            .map_err(sqlite_err("query artifact detail"))?;
        rows.next()
            .transpose()
            .map_err(sqlite_err("read artifact detail"))
    }

    pub fn lookup_artifact(&self, term: &str) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select series_id, artifact_code, artifact_type, owner, latest_version_id, comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json from domain_artifacts where lower(series_id) = lower(?1) or lower(artifact_code) = lower(?1) limit 1")
            .map_err(sqlite_err("prepare artifact lookup"))?;
        let mut rows = stmt
            .query_map([term], artifact_from_row)
            .map_err(sqlite_err("query artifact lookup"))?;
        rows.next()
            .transpose()
            .map_err(sqlite_err("read artifact lookup"))
    }

    pub fn count_artifacts(&self, artifact_type: Option<u64>) -> paperproof_sdk_rs::Result<u64> {
        let conn = self.connection()?;
        let count = if let Some(kind) = artifact_type {
            let mut stmt = conn
                .prepare("select count(*) from domain_artifacts where artifact_type = ?1")
                .map_err(sqlite_err("prepare artifact count"))?;
            stmt.query_row([u64_to_i64(kind)?], |row| row.get::<_, i64>(0))
                .map_err(sqlite_err("query artifact count"))?
        } else {
            scalar_i64(&conn, "select count(*) from domain_artifacts")?
        };
        u64::try_from(count).map_err(|err| {
            paperproof_sdk_rs::PaperProofError::invalid_input("artifact count", err.to_string())
        })
    }

    pub fn count_comments(&self, series_id: &str) -> paperproof_sdk_rs::Result<u64> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select count(*) from domain_comments where series_id = ?1")
            .map_err(sqlite_err("prepare comment count"))?;
        let count = stmt
            .query_row([series_id], |row| row.get::<_, i64>(0))
            .map_err(sqlite_err("query comment count"))?;
        u64::try_from(count).map_err(|err| {
            paperproof_sdk_rs::PaperProofError::invalid_input("comment count", err.to_string())
        })
    }

    pub fn versions(&self, series_id: &str) -> paperproof_sdk_rs::Result<Vec<VersionRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select version_id, series_id, artifact_type, version, content_hash, walrus_blob_id, content_type, created_at, raw_json from domain_versions where series_id = ?1 order by version asc, created_at asc")
            .map_err(sqlite_err("prepare versions"))?;
        stmt.query_map([series_id], version_from_row)
            .map_err(sqlite_err("query versions"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_err("read versions"))
    }

    pub fn comments(
        &self,
        series_id: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<CommentRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select tree_id, comment_id, parent_comment_id, series_id, author, content_mode, status, created_at, updated_at, raw_json from domain_comments where series_id = ?1 order by parent_comment_id asc, comment_id asc limit ?2 offset ?3")
            .map_err(sqlite_err("prepare comments"))?;
        stmt.query_map(
            rusqlite::params![series_id, u64_to_i64(limit)?, u64_to_i64(offset)?],
            comment_from_row,
        )
        .map_err(sqlite_err("query comments"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read comments"))
    }

    pub fn proposals(
        &self,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceProposalRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select proposal_id, proposal_object_id, proposer, title, action_type, proposal_type, status, yes_votes, no_votes, created_at, updated_at, raw_json from domain_governance_proposals order by proposal_id desc limit ?1 offset ?2")
            .map_err(sqlite_err("prepare proposals"))?;
        stmt.query_map(
            rusqlite::params![u64_to_i64(limit)?, u64_to_i64(offset)?],
            proposal_from_row,
        )
        .map_err(sqlite_err("query proposals"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read proposals"))
    }

    pub fn votes_for_address(
        &self,
        address: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceVoteRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select proposal_id, voter, side, voting_power, claimed, created_at, updated_at, raw_json from domain_votes where lower(voter) = lower(?1) order by created_at desc limit ?2 offset ?3")
            .map_err(sqlite_err("prepare votes"))?;
        stmt.query_map(
            rusqlite::params![address, u64_to_i64(limit)?, u64_to_i64(offset)?],
            vote_from_row,
        )
        .map_err(sqlite_err("query votes"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read votes"))
    }

    pub fn artifacts_by_owner(
        &self,
        owner: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select series_id, artifact_code, artifact_type, owner, latest_version_id, comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json from domain_artifacts where lower(owner) = lower(?1) order by coalesce(updated_at, published_at) desc limit ?2 offset ?3")
            .map_err(sqlite_err("prepare owner artifacts"))?;
        stmt.query_map(
            rusqlite::params![owner, u64_to_i64(limit)?, u64_to_i64(offset)?],
            artifact_from_row,
        )
        .map_err(sqlite_err("query owner artifacts"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read owner artifacts"))
    }

    pub fn airdrop_rows(&self) -> paperproof_sdk_rs::Result<Vec<AirdropRow>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare("select address, published_artifacts, versions_added, comments, votes, likes, score, reasons_json from domain_airdrop_scores order by score desc, address asc")
            .map_err(sqlite_err("prepare airdrop"))?;
        stmt.query_map([], |row| {
            let reasons: String = row.get(7)?;
            Ok(AirdropRow {
                address: row.get(0)?,
                published_artifacts: i64_to_u64(row.get(1)?)?,
                versions_added: i64_to_u64(row.get(2)?)?,
                comments: i64_to_u64(row.get(3)?)?,
                votes: i64_to_u64(row.get(4)?)?,
                likes: i64_to_u64(row.get(5)?)?,
                score: i64_to_u64(row.get(6)?)?,
                reasons: serde_json::from_str(&reasons).unwrap_or_default(),
            })
        })
        .map_err(sqlite_err("query airdrop"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read airdrop"))
    }

    pub fn search_artifacts(
        &self,
        term: &str,
        artifact_type: Option<u64>,
        owner: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let conn = self.connection()?;
        let pattern = format!("%{}%", term.trim());
        let mut stmt = conn
            .prepare("select series_id, artifact_code, artifact_type, owner, latest_version_id, comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json
                from domain_artifacts
                where (artifact_code like ?1 or title like ?1 or owner like ?1 or series_id like ?1 or raw_json like ?1)
                  and (?2 is null or artifact_type = ?2)
                  and (?3 is null or lower(owner) = lower(?3))
                order by
                  case
                    when artifact_code = ?4 then 0
                    when title = ?4 then 1
                    when artifact_code like ?1 then 2
                    else 3
                  end,
                  coalesce(updated_at, published_at) desc
                limit ?5 offset ?6")
            .map_err(sqlite_err("prepare artifact search"))?;
        stmt.query_map(
            rusqlite::params![
                pattern,
                opt_u64_to_i64(artifact_type)?,
                owner,
                term.trim(),
                u64_to_i64(limit)?,
                u64_to_i64(offset)?
            ],
            artifact_from_row,
        )
        .map_err(sqlite_err("query artifact search"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read artifact search"))
    }

    pub fn activity(
        &self,
        actor: Option<&str>,
        series_id: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ActivityRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn
            .prepare(
                "select event_key, kind, actor, series_id, proposal_id, tree_id, created_at, raw_json
                 from domain_activity
                 where (?1 is null or lower(actor) = lower(?1))
                   and (?2 is null or series_id = ?2)
                 order by created_at desc, event_key desc
                 limit ?3 offset ?4",
            )
            .map_err(sqlite_err("prepare activity"))?;
        stmt.query_map(
            rusqlite::params![actor, series_id, u64_to_i64(limit)?, u64_to_i64(offset)?],
            activity_from_row,
        )
        .map_err(sqlite_err("query activity"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite_err("read activity"))
    }

    fn connection(&self) -> paperproof_sdk_rs::Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(sqlite_err("open normalized database"))?;
        ensure_sqlite_schema(&conn)?;
        Ok(conn)
    }
}

#[cfg(feature = "postgres")]
impl PostgresNormalizedQuery {
    pub async fn connect(connection_string: &str) -> paperproof_sdk_rs::Result<Self> {
        let (client, connection) =
            tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
                .await
                .map_err(postgres_err("connect postgres query"))?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                eprintln!("paperproof postgres query connection closed: {error}");
            }
        });
        client
            .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
            .await
            .map_err(postgres_err("ensure postgres query schema"))?;
        Ok(Self {
            client: std::sync::Arc::new(tokio::sync::Mutex::new(client)),
        })
    }

    pub async fn summary(&self) -> paperproof_sdk_rs::Result<crate::analytics::AnalyticsSummary> {
        let client = self.client.lock().await;
        Ok(crate::analytics::AnalyticsSummary {
            total_artifacts: pg_count(&client, "domain_artifacts").await?,
            total_versions: pg_count(&client, "domain_versions").await?,
            total_comments: pg_count(&client, "domain_comments").await?,
            total_likes: pg_scalar_i64(
                &client,
                "select coalesce(sum(likes), 0) from domain_airdrop_scores",
            )
            .await? as u64,
            total_proposals: pg_count(&client, "domain_governance_proposals").await?,
            total_votes: pg_count(&client, "domain_votes").await?,
            last_checkpoint: pg_scalar_i64(
                &client,
                "select coalesce(max(checkpoint), 0) from paperproof_events",
            )
            .await
            .ok()
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0),
            content_refs_pending: pg_scalar_i64(
                &client,
                "select count(*) from paperproof_content_refs where status = 'pending'",
            )
            .await? as u64,
            content_cache_verified: pg_scalar_i64(
                &client,
                "select count(*) from paperproof_content_cache where status = 'verified'",
            )
            .await? as u64,
            top_contributors: pg_top_contributors(&client, 10).await?,
            artifact_types: pg_artifact_type_summary(&client).await?,
        })
    }

    pub async fn recent_artifacts(
        &self,
        artifact_type: Option<u64>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let client = self.client.lock().await;
        let kind = opt_u64_to_i64_pg(artifact_type)?;
        let rows = client
            .query(
                "select series_id, artifact_code, artifact_type, owner, latest_version_id,
                        comments_tree_id, likes_book_id, title, status,
                        published_at::text, updated_at::text, raw_json
                 from domain_artifacts
                 where ($1::bigint is null or artifact_type = $1)
                 order by coalesce(updated_at, published_at) desc
                 limit $2 offset $3",
                &[&kind, &u64_to_i64_pg(limit)?, &u64_to_i64_pg(offset)?],
            )
            .await
            .map_err(postgres_err("postgres query artifacts"))?;
        rows.iter().map(pg_artifact_from_row).collect()
    }

    pub async fn artifact_detail(
        &self,
        series_id: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "select series_id, artifact_code, artifact_type, owner, latest_version_id,
                        comments_tree_id, likes_book_id, title, status,
                        published_at::text, updated_at::text, raw_json
                 from domain_artifacts where series_id = $1",
                &[&series_id],
            )
            .await
            .map_err(postgres_err("postgres artifact detail"))?;
        row.as_ref().map(pg_artifact_from_row).transpose()
    }

    pub async fn lookup_artifact(
        &self,
        term: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "select series_id, artifact_code, artifact_type, owner, latest_version_id,
                        comments_tree_id, likes_book_id, title, status,
                        published_at::text, updated_at::text, raw_json
                 from domain_artifacts
                 where lower(series_id) = lower($1) or lower(artifact_code) = lower($1)
                 limit 1",
                &[&term],
            )
            .await
            .map_err(postgres_err("postgres artifact lookup"))?;
        row.as_ref().map(pg_artifact_from_row).transpose()
    }

    pub async fn count_artifacts(
        &self,
        artifact_type: Option<u64>,
    ) -> paperproof_sdk_rs::Result<u64> {
        let client = self.client.lock().await;
        let kind = opt_u64_to_i64_pg(artifact_type)?;
        let row = client
            .query_one(
                "select count(*) from domain_artifacts where ($1::bigint is null or artifact_type = $1)",
                &[&kind],
            )
            .await
            .map_err(postgres_err("postgres artifact count"))?;
        i64_to_u64_pg(row.get(0))
    }

    pub async fn count_comments(&self, series_id: &str) -> paperproof_sdk_rs::Result<u64> {
        let client = self.client.lock().await;
        let row = client
            .query_one(
                "select count(*) from domain_comments where series_id = $1",
                &[&series_id],
            )
            .await
            .map_err(postgres_err("postgres comment count"))?;
        i64_to_u64_pg(row.get(0))
    }

    pub async fn versions(&self, series_id: &str) -> paperproof_sdk_rs::Result<Vec<VersionRecord>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select version_id, series_id, artifact_type, version, content_hash,
                        walrus_blob_id, content_type, created_at::text, raw_json
                 from domain_versions
                 where series_id = $1
                 order by version asc, created_at asc",
                &[&series_id],
            )
            .await
            .map_err(postgres_err("postgres versions"))?;
        rows.iter().map(pg_version_from_row).collect()
    }

    pub async fn comments(
        &self,
        series_id: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<CommentRecord>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select tree_id, comment_id, parent_comment_id, series_id, author,
                        content_mode, status, created_at::text, updated_at::text, raw_json
                 from domain_comments
                 where series_id = $1
                 order by parent_comment_id asc, comment_id asc
                 limit $2 offset $3",
                &[&series_id, &u64_to_i64_pg(limit)?, &u64_to_i64_pg(offset)?],
            )
            .await
            .map_err(postgres_err("postgres comments"))?;
        rows.iter().map(pg_comment_from_row).collect()
    }

    pub async fn proposals(
        &self,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceProposalRecord>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select proposal_id, proposal_object_id, proposer, title, action_type,
                        proposal_type, status, yes_votes, no_votes,
                        created_at::text, updated_at::text, raw_json
                 from domain_governance_proposals
                 order by proposal_id desc
                 limit $1 offset $2",
                &[&u64_to_i64_pg(limit)?, &u64_to_i64_pg(offset)?],
            )
            .await
            .map_err(postgres_err("postgres proposals"))?;
        rows.iter().map(pg_proposal_from_row).collect()
    }

    pub async fn votes_for_address(
        &self,
        address: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceVoteRecord>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select proposal_id, voter, side, voting_power, claimed,
                        created_at::text, updated_at::text, raw_json
                 from domain_votes
                 where lower(voter) = lower($1)
                 order by created_at desc
                 limit $2 offset $3",
                &[&address, &u64_to_i64_pg(limit)?, &u64_to_i64_pg(offset)?],
            )
            .await
            .map_err(postgres_err("postgres votes"))?;
        rows.iter().map(pg_vote_from_row).collect()
    }

    pub async fn artifacts_by_owner(
        &self,
        owner: &str,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select series_id, artifact_code, artifact_type, owner, latest_version_id,
                        comments_tree_id, likes_book_id, title, status,
                        published_at::text, updated_at::text, raw_json
                 from domain_artifacts
                 where lower(owner) = lower($1)
                 order by coalesce(updated_at, published_at) desc
                 limit $2 offset $3",
                &[&owner, &u64_to_i64_pg(limit)?, &u64_to_i64_pg(offset)?],
            )
            .await
            .map_err(postgres_err("postgres owner artifacts"))?;
        rows.iter().map(pg_artifact_from_row).collect()
    }

    pub async fn airdrop_rows(&self) -> paperproof_sdk_rs::Result<Vec<AirdropRow>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select address, published_artifacts, versions_added, comments, votes,
                        likes, score, reasons_json
                 from domain_airdrop_scores
                 order by score desc, address asc",
                &[],
            )
            .await
            .map_err(postgres_err("postgres airdrop"))?;
        rows.iter().map(pg_airdrop_from_row).collect()
    }

    pub async fn search_artifacts(
        &self,
        term: &str,
        artifact_type: Option<u64>,
        owner: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        let client = self.client.lock().await;
        let pattern = format!("%{}%", term.trim());
        let kind = opt_u64_to_i64_pg(artifact_type)?;
        let rows = client
            .query(
                "select series_id, artifact_code, artifact_type, owner, latest_version_id,
                        comments_tree_id, likes_book_id, title, status,
                        published_at::text, updated_at::text, raw_json
                 from domain_artifacts
                 where (
                    artifact_code ilike $1 or title ilike $1 or owner ilike $1 or
                    series_id ilike $1 or raw_json::text ilike $1
                 )
                   and ($2::bigint is null or artifact_type = $2)
                   and ($3::text is null or lower(owner) = lower($3))
                 order by
                    case
                        when artifact_code = $4 then 0
                        when title = $4 then 1
                        when artifact_code ilike $1 then 2
                        else 3
                    end,
                    coalesce(updated_at, published_at) desc
                 limit $5 offset $6",
                &[
                    &pattern,
                    &kind,
                    &owner,
                    &term.trim(),
                    &u64_to_i64_pg(limit)?,
                    &u64_to_i64_pg(offset)?,
                ],
            )
            .await
            .map_err(postgres_err("postgres artifact search"))?;
        rows.iter().map(pg_artifact_from_row).collect()
    }

    pub async fn activity(
        &self,
        actor: Option<&str>,
        series_id: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ActivityRecord>> {
        let client = self.client.lock().await;
        let rows = client
            .query(
                "select event_key, kind, actor, series_id, proposal_id, tree_id,
                        created_at::text, raw_json
                 from domain_activity
                 where ($1::text is null or lower(actor) = lower($1))
                   and ($2::text is null or series_id = $2)
                 order by created_at desc, event_key desc
                 limit $3 offset $4",
                &[
                    &actor,
                    &series_id,
                    &u64_to_i64_pg(limit)?,
                    &u64_to_i64_pg(offset)?,
                ],
            )
            .await
            .map_err(postgres_err("postgres activity"))?;
        rows.iter().map(pg_activity_from_row).collect()
    }
}

#[cfg(not(feature = "sqlite"))]
impl NormalizedQuery {
    pub fn sqlite(_db_path: impl Into<String>) -> Self {
        Self
    }

    pub fn summary(&self) -> paperproof_sdk_rs::Result<crate::analytics::AnalyticsSummary> {
        Err(sqlite_required())
    }

    pub fn recent_artifacts(
        &self,
        _artifact_type: Option<u64>,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        Err(sqlite_required())
    }

    pub fn artifact_detail(
        &self,
        _series_id: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        Err(sqlite_required())
    }

    pub fn lookup_artifact(
        &self,
        _term: &str,
    ) -> paperproof_sdk_rs::Result<Option<ArtifactRecord>> {
        Err(sqlite_required())
    }

    pub fn count_artifacts(&self, _artifact_type: Option<u64>) -> paperproof_sdk_rs::Result<u64> {
        Err(sqlite_required())
    }

    pub fn count_comments(&self, _series_id: &str) -> paperproof_sdk_rs::Result<u64> {
        Err(sqlite_required())
    }

    pub fn versions(&self, _series_id: &str) -> paperproof_sdk_rs::Result<Vec<VersionRecord>> {
        Err(sqlite_required())
    }

    pub fn comments(
        &self,
        _series_id: &str,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<CommentRecord>> {
        Err(sqlite_required())
    }

    pub fn proposals(
        &self,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceProposalRecord>> {
        Err(sqlite_required())
    }

    pub fn votes_for_address(
        &self,
        _address: &str,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<GovernanceVoteRecord>> {
        Err(sqlite_required())
    }

    pub fn artifacts_by_owner(
        &self,
        _owner: &str,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        Err(sqlite_required())
    }

    pub fn airdrop_rows(&self) -> paperproof_sdk_rs::Result<Vec<AirdropRow>> {
        Err(sqlite_required())
    }

    pub fn search_artifacts(
        &self,
        _term: &str,
        _artifact_type: Option<u64>,
        _owner: Option<&str>,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ArtifactRecord>> {
        Err(sqlite_required())
    }

    pub fn activity(
        &self,
        _actor: Option<&str>,
        _series_id: Option<&str>,
        _limit: u64,
        _offset: u64,
    ) -> paperproof_sdk_rs::Result<Vec<ActivityRecord>> {
        Err(sqlite_required())
    }
}

#[cfg(feature = "sqlite")]
pub fn apply_normalized_batch_sqlite(
    db_path: &str,
    batch: &IndexerEventBatch,
) -> paperproof_sdk_rs::Result<()> {
    let conn =
        rusqlite::Connection::open(db_path).map_err(sqlite_err("open normalized database"))?;
    ensure_sqlite_schema(&conn)?;
    for event in &batch.accepted {
        apply_event(&conn, event)?;
    }
    Ok(())
}

#[cfg(feature = "postgres")]
pub async fn apply_normalized_batch_postgres(
    connection_string: &str,
    batch: &IndexerEventBatch,
) -> paperproof_sdk_rs::Result<()> {
    let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
        .await
        .map_err(postgres_err("connect normalized postgres"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("paperproof normalized postgres connection closed: {error}");
        }
    });
    client
        .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
        .await
        .map_err(postgres_err("ensure postgres normalized schema"))?;
    for event in &batch.accepted {
        apply_event_postgres(&client, event).await?;
    }
    Ok(())
}

#[cfg(not(feature = "postgres"))]
pub async fn apply_normalized_batch_postgres(
    _connection_string: &str,
    _batch: &IndexerEventBatch,
) -> paperproof_sdk_rs::Result<()> {
    Err(PaperProofError::invalid_input(
        "postgres",
        "Postgres normalized projection requires --features postgres",
    ))
}

#[cfg(feature = "sqlite")]
pub fn rebuild_normalized_from_sqlite_raw(
    db_path: &str,
    clear_existing: bool,
) -> paperproof_sdk_rs::Result<RebuildReport> {
    let conn =
        rusqlite::Connection::open(db_path).map_err(sqlite_err("open normalized database"))?;
    ensure_sqlite_schema(&conn)?;
    if clear_existing {
        clear_normalized_sqlite(&conn)?;
    }
    let mut stmt = conn
        .prepare(
            "select event_key, checkpoint, transaction_digest, event_seq, package_id, module,
                    event_type, kind, sender, timestamp_ms, parsed_json
             from paperproof_events
             order by checkpoint asc, transaction_digest asc, event_seq asc",
        )
        .map_err(sqlite_err("prepare raw event replay"))?;
    let mut rows = stmt
        .query([])
        .map_err(sqlite_err("query raw event replay"))?;
    let mut report = RebuildReport {
        source: db_path.to_string(),
        normalized_tables_cleared: clear_existing,
        ..Default::default()
    };
    while let Some(row) = rows.next().map_err(sqlite_err("read raw event replay"))? {
        report.events_seen += 1;
        let event = indexed_event_from_sqlite_row(row).map_err(sqlite_err("decode raw event"))?;
        apply_event(&conn, &event)?;
        report.events_applied += 1;
    }
    Ok(report)
}

#[cfg(feature = "postgres")]
pub async fn rebuild_normalized_from_postgres_raw(
    connection_string: &str,
    clear_existing: bool,
) -> paperproof_sdk_rs::Result<RebuildReport> {
    let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
        .await
        .map_err(postgres_err("connect postgres rebuild"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("paperproof postgres rebuild connection closed: {error}");
        }
    });
    client
        .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
        .await
        .map_err(postgres_err("ensure postgres rebuild schema"))?;
    if clear_existing {
        clear_normalized_postgres(&client).await?;
    }
    let rows = client
        .query(
            "select event_key, checkpoint, transaction_digest, event_seq, package_id, module,
                    event_type, kind, sender, timestamp_ms, parsed_json
             from paperproof_events
             order by checkpoint asc, transaction_digest asc, event_seq asc",
            &[],
        )
        .await
        .map_err(postgres_err("query postgres raw replay"))?;
    let mut report = RebuildReport {
        source: connection_string.to_string(),
        normalized_tables_cleared: clear_existing,
        ..Default::default()
    };
    for row in rows {
        report.events_seen += 1;
        let event = indexed_event_from_postgres_row(&row)?;
        apply_event_postgres(&client, &event).await?;
        report.events_applied += 1;
    }
    Ok(report)
}

#[cfg(not(feature = "postgres"))]
pub async fn rebuild_normalized_from_postgres_raw(
    _connection_string: &str,
    _clear_existing: bool,
) -> paperproof_sdk_rs::Result<RebuildReport> {
    Err(PaperProofError::invalid_input(
        "postgres",
        "Postgres normalized rebuild requires --features postgres",
    ))
}

#[cfg(not(feature = "sqlite"))]
pub fn rebuild_normalized_from_sqlite_raw(
    _db_path: &str,
    _clear_existing: bool,
) -> paperproof_sdk_rs::Result<RebuildReport> {
    Err(PaperProofError::invalid_input(
        "sqlite",
        "SQLite normalized rebuild requires --features sqlite",
    ))
}

#[cfg(not(feature = "sqlite"))]
pub fn apply_normalized_batch_sqlite(
    _db_path: &str,
    _batch: &IndexerEventBatch,
) -> paperproof_sdk_rs::Result<()> {
    Err(PaperProofError::invalid_input(
        "sqlite",
        "normalized SQLite projection requires --features sqlite",
    ))
}

#[cfg(feature = "sqlite")]
pub fn export_airdrop_snapshot(
    db_path: &str,
    output_path: &str,
    format: AirdropFormat,
) -> paperproof_sdk_rs::Result<Vec<AirdropRow>> {
    let rows = NormalizedQuery::sqlite(db_path).airdrop_rows()?;
    let text = match format {
        AirdropFormat::Json => serde_json::to_string_pretty(&rows)?,
        AirdropFormat::Csv => {
            let mut out = String::from(
                "address,published_artifacts,versions_added,comments,votes,likes,score,reasons\n",
            );
            for row in &rows {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    row.address,
                    row.published_artifacts,
                    row.versions_added,
                    row.comments,
                    row.votes,
                    row.likes,
                    row.score,
                    csv_escape(&row.reasons.join("; "))
                ));
            }
            out
        }
    };
    if let Some(parent) = std::path::Path::new(output_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|err| {
            PaperProofError::network(parent.display().to_string(), err.to_string())
        })?;
    }
    std::fs::write(output_path, text)
        .map_err(|err| PaperProofError::network(output_path, err.to_string()))?;
    Ok(rows)
}

#[cfg(not(feature = "sqlite"))]
pub fn export_airdrop_snapshot(
    _db_path: &str,
    _output_path: &str,
    _format: AirdropFormat,
) -> paperproof_sdk_rs::Result<Vec<AirdropRow>> {
    Err(PaperProofError::invalid_input(
        "sqlite",
        "airdrop export requires --features sqlite",
    ))
}

#[cfg(feature = "sqlite")]
fn apply_event(
    conn: &rusqlite::Connection,
    event: &IndexedPaperProofEvent,
) -> paperproof_sdk_rs::Result<()> {
    let fields = &event.event.parsed_json;
    let raw = serde_json::to_string(fields)?;
    let event_key = event.id.key();
    let created_at = event.event.timestamp_ms.clone();
    match event.kind {
        PaperProofEventKind::ArtifactPublished => {
            let Some(series_id) = str_field(fields, "series_id") else {
                return Ok(());
            };
            let version_id = str_field(fields, "version_id");
            conn.execute(
                "insert into domain_artifacts (
                    series_id, artifact_code, artifact_type, owner, latest_version_id,
                    comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json
                ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, current_timestamp, ?11)
                on conflict(series_id) do update set
                    artifact_code = coalesce(excluded.artifact_code, domain_artifacts.artifact_code),
                    artifact_type = coalesce(excluded.artifact_type, domain_artifacts.artifact_type),
                    owner = coalesce(excluded.owner, domain_artifacts.owner),
                    latest_version_id = coalesce(excluded.latest_version_id, domain_artifacts.latest_version_id),
                    comments_tree_id = coalesce(excluded.comments_tree_id, domain_artifacts.comments_tree_id),
                    likes_book_id = coalesce(excluded.likes_book_id, domain_artifacts.likes_book_id),
                    title = coalesce(excluded.title, domain_artifacts.title),
                    updated_at = current_timestamp,
                    raw_json = excluded.raw_json",
                rusqlite::params![
                    series_id,
                    str_field(fields, "artifact_code"),
                    opt_u64_to_i64(u64_field(fields, "artifact_type"))?,
                    str_field(fields, "owner")
                        .or_else(|| str_field(fields, "publisher"))
                        .or_else(|| Some(event.event.sender.clone())),
                    version_id,
                    str_field(fields, "comments_tree_id"),
                    str_field(fields, "likes_book_id"),
                    str_field(fields, "title").or_else(|| str_field(fields, "artifact_code")),
                    opt_u64_to_i64(u64_field(fields, "status"))?,
                    created_at,
                    raw,
                ],
            )
            .map_err(sqlite_err("upsert artifact"))?;
            if let Some(version_id) = version_id {
                insert_version(conn, fields, &raw, &created_at, &series_id, &version_id)?;
            }
            bump_score(
                conn,
                &event.event.sender,
                "published_artifacts",
                10,
                "published artifact",
            )?;
        }
        PaperProofEventKind::ArtifactVersionAdded => {
            let Some(series_id) = str_field(fields, "series_id") else {
                return Ok(());
            };
            let Some(version_id) =
                str_field(fields, "version_id").or_else(|| str_field(fields, "new_version_id"))
            else {
                return Ok(());
            };
            insert_version(conn, fields, &raw, &created_at, &series_id, &version_id)?;
            conn.execute(
                "update domain_artifacts set latest_version_id = ?1, updated_at = current_timestamp where series_id = ?2",
                rusqlite::params![version_id, series_id],
            )
            .map_err(sqlite_err("update artifact latest version"))?;
            bump_score(
                conn,
                &event.event.sender,
                "versions_added",
                3,
                "added version",
            )?;
        }
        PaperProofEventKind::OwnerTransferred => {
            if let Some(series_id) = str_field(fields, "series_id") {
                conn.execute(
                    "update domain_artifacts set owner = ?1, updated_at = current_timestamp where series_id = ?2",
                    rusqlite::params![str_field(fields, "new_owner"), series_id],
                )
                .map_err(sqlite_err("update owner"))?;
            }
        }
        PaperProofEventKind::CommentAdded => {
            let Some(tree_id) = str_field(fields, "tree_id") else {
                return Ok(());
            };
            let Some(comment_id) = u64_field(fields, "comment_id") else {
                return Ok(());
            };
            let series_id = series_for_tree(conn, &tree_id)?
                .or_else(|| str_field(fields, "series_id"))
                .or_else(|| str_field(fields, "target_series_id"));
            conn.execute(
                "insert into domain_comments (
                    tree_id, comment_id, parent_comment_id, series_id, author, content_mode,
                    status, created_at, updated_at, raw_json
                ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp, ?9)
                on conflict(tree_id, comment_id) do update set
                    parent_comment_id = excluded.parent_comment_id,
                    series_id = coalesce(excluded.series_id, domain_comments.series_id),
                    author = coalesce(excluded.author, domain_comments.author),
                    content_mode = coalesce(excluded.content_mode, domain_comments.content_mode),
                    updated_at = current_timestamp,
                    raw_json = excluded.raw_json",
                rusqlite::params![
                    tree_id,
                    u64_to_i64(comment_id)?,
                    opt_u64_to_i64(u64_field(fields, "parent_comment_id"))?,
                    series_id,
                    str_field(fields, "author")
                        .or_else(|| str_field(fields, "commenter"))
                        .or_else(|| Some(event.event.sender.clone())),
                    opt_u64_to_i64(u64_field(fields, "content_mode"))?,
                    opt_u64_to_i64(u64_field(fields, "status"))?,
                    created_at,
                    raw,
                ],
            )
            .map_err(sqlite_err("upsert comment"))?;
            bump_score(conn, &event.event.sender, "comments", 1, "commented")?;
        }
        PaperProofEventKind::CommentStatusChanged => {
            if let (Some(tree_id), Some(comment_id)) = (
                str_field(fields, "tree_id"),
                u64_field(fields, "comment_id"),
            ) {
                conn.execute(
                    "update domain_comments set status = ?1, updated_at = current_timestamp, raw_json = ?2 where tree_id = ?3 and comment_id = ?4",
                    rusqlite::params![
                        opt_u64_to_i64(u64_field(fields, "new_status"))?,
                        raw,
                        tree_id,
                        u64_to_i64(comment_id)?,
                    ],
                )
                .map_err(sqlite_err("update comment status"))?;
            }
        }
        PaperProofEventKind::PaperLiked => {
            bump_score(conn, &event.event.sender, "likes", 1, "liked artifact")?;
        }
        PaperProofEventKind::ProposalCreated => {
            let Some(proposal_id) = u64_field(fields, "proposal_id") else {
                return Ok(());
            };
            conn.execute(
                "insert into domain_governance_proposals (
                    proposal_id, proposal_object_id, proposer, title, action_type, proposal_type,
                    status, yes_votes, no_votes, created_at, updated_at, raw_json
                ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, current_timestamp, ?11)
                on conflict(proposal_id) do update set
                    proposal_object_id = coalesce(excluded.proposal_object_id, domain_governance_proposals.proposal_object_id),
                    proposer = coalesce(excluded.proposer, domain_governance_proposals.proposer),
                    title = coalesce(excluded.title, domain_governance_proposals.title),
                    action_type = coalesce(excluded.action_type, domain_governance_proposals.action_type),
                    proposal_type = coalesce(excluded.proposal_type, domain_governance_proposals.proposal_type),
                    updated_at = current_timestamp,
                    raw_json = excluded.raw_json",
                rusqlite::params![
                    u64_to_i64(proposal_id)?,
                    str_field(fields, "proposal_object_id"),
                    str_field(fields, "proposer").or_else(|| Some(event.event.sender.clone())),
                    str_field(fields, "title"),
                    opt_u64_to_i64(u64_field(fields, "action_type"))?,
                    opt_u64_to_i64(u64_field(fields, "proposal_type"))?,
                    opt_u64_to_i64(u64_field(fields, "status"))?,
                    str_field(fields, "yes_votes"),
                    str_field(fields, "no_votes"),
                    created_at,
                    raw,
                ],
            )
            .map_err(sqlite_err("upsert proposal"))?;
        }
        PaperProofEventKind::ProposalVoted => {
            let Some(proposal_id) = u64_field(fields, "proposal_id") else {
                return Ok(());
            };
            let voter = str_field(fields, "voter").unwrap_or_else(|| event.event.sender.clone());
            conn.execute(
                "insert into domain_votes (
                    proposal_id, voter, side, voting_power, claimed, created_at, updated_at, raw_json
                ) values (?1, ?2, ?3, ?4, 0, ?5, current_timestamp, ?6)
                on conflict(proposal_id, voter) do update set
                    side = excluded.side,
                    voting_power = excluded.voting_power,
                    updated_at = current_timestamp,
                    raw_json = excluded.raw_json",
                rusqlite::params![
                    u64_to_i64(proposal_id)?,
                    voter,
                    opt_u64_to_i64(u64_field(fields, "side"))?,
                    str_field(fields, "voting_power")
                        .or_else(|| u64_field(fields, "voting_power").map(|v| v.to_string())),
                    created_at,
                    raw,
                ],
            )
            .map_err(sqlite_err("upsert vote"))?;
            bump_score(conn, &event.event.sender, "votes", 5, "voted")?;
        }
        PaperProofEventKind::ProposalFinalized | PaperProofEventKind::ProposalExpired => {
            if let Some(proposal_id) = u64_field(fields, "proposal_id") {
                conn.execute(
                    "update domain_governance_proposals set status = ?1, yes_votes = coalesce(?2, yes_votes), no_votes = coalesce(?3, no_votes), updated_at = current_timestamp, raw_json = ?4 where proposal_id = ?5",
                    rusqlite::params![
                        opt_u64_to_i64(u64_field(fields, "status"))?,
                        str_field(fields, "yes_votes"),
                        str_field(fields, "no_votes"),
                        raw,
                        u64_to_i64(proposal_id)?,
                    ],
                )
                .map_err(sqlite_err("update proposal status"))?;
            }
        }
        PaperProofEventKind::VoteClaimed => {
            if let (Some(proposal_id), Some(voter)) =
                (u64_field(fields, "proposal_id"), str_field(fields, "voter"))
            {
                conn.execute(
                    "update domain_votes set claimed = 1, updated_at = current_timestamp, raw_json = ?1 where proposal_id = ?2 and lower(voter) = lower(?3)",
                    rusqlite::params![raw, u64_to_i64(proposal_id)?, voter],
                )
                .map_err(sqlite_err("mark claimed"))?;
            }
        }
        _ => {}
    }
    insert_activity(conn, event, &event_key, &created_at)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn ensure_sqlite_schema(conn: &rusqlite::Connection) -> paperproof_sdk_rs::Result<()> {
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure normalized schema"))
}

#[cfg(feature = "sqlite")]
fn indexed_event_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<IndexedPaperProofEvent> {
    let event_key: String = row.get(0)?;
    let checkpoint: Option<i64> = row.get(1)?;
    let transaction_digest: Option<String> = row.get(2)?;
    let event_seq: Option<i64> = row.get(3)?;
    let package_id: String = row.get(4)?;
    let module: String = row.get(5)?;
    let event_type: String = row.get(6)?;
    let kind: String = row.get(7)?;
    let sender: Option<String> = row.get(8)?;
    let timestamp_ms: Option<i64> = row.get(9)?;
    let parsed_json_text: String = row.get(10)?;
    let parsed_json = serde_json::from_str(&parsed_json_text).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let kind = parse_kind(&kind);
    let event = paperproof_sdk_rs::events::SuiEventEnvelope {
        id: Some(serde_json::json!({
            "eventKey": event_key,
            "txDigest": transaction_digest,
            "eventSeq": event_seq,
            "checkpoint": checkpoint
        })),
        package_id: package_id.clone(),
        transaction_module: module.clone(),
        sender: sender.unwrap_or_default(),
        event_type: event_type.clone(),
        parsed_json,
        bcs: None,
        timestamp_ms: timestamp_ms.map(|value| value.to_string()),
    };
    Ok(IndexedPaperProofEvent {
        id: paperproof_sdk_rs::EventId {
            checkpoint: checkpoint.and_then(|value| u64::try_from(value).ok()),
            transaction_digest,
            event_seq: event_seq.and_then(|value| u64::try_from(value).ok()),
            package_id,
            module,
            event_type,
        },
        verification: paperproof_sdk_rs::events_trust::verification_report_from_canonical_check(
            &event,
            &paperproof_sdk_rs::MAINNET_DEPLOYMENT,
            paperproof_sdk_rs::EventTrustLevel::Canonical,
        ),
        trust: paperproof_sdk_rs::events_trust::EventTrustResult {
            trusted: true,
            reason: None,
            status: paperproof_sdk_rs::events_trust::EventVerificationStatus::Canonical,
            issues: vec![],
        },
        kind,
        event,
    })
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn parse_kind(kind: &str) -> PaperProofEventKind {
    match kind {
        "RootCreated" => PaperProofEventKind::RootCreated,
        "TypeRegistryCreated" => PaperProofEventKind::TypeRegistryCreated,
        "TypeIndexCreated" => PaperProofEventKind::TypeIndexCreated,
        "TreeCreated" => PaperProofEventKind::TreeCreated,
        "GovernanceVaultCreated" => PaperProofEventKind::GovernanceVaultCreated,
        "FeeManagerCreated" => PaperProofEventKind::FeeManagerCreated,
        "GovernanceConfigCreated" => PaperProofEventKind::GovernanceConfigCreated,
        "GovernanceConfigBound" => PaperProofEventKind::GovernanceConfigBound,
        "ArtifactPublished" => PaperProofEventKind::ArtifactPublished,
        "ArtifactVersionAdded" => PaperProofEventKind::ArtifactVersionAdded,
        "SeriesMetadataUpdated" => PaperProofEventKind::SeriesMetadataUpdated,
        "ArtifactTypeStatusChanged" => PaperProofEventKind::ArtifactTypeStatusChanged,
        "CommentAdded" => PaperProofEventKind::CommentAdded,
        "CommentStatusChanged" => PaperProofEventKind::CommentStatusChanged,
        "TreeStatusChanged" => PaperProofEventKind::TreeStatusChanged,
        "CommentsTreeMigrated" => PaperProofEventKind::CommentsTreeMigrated,
        "PaperLiked" => PaperProofEventKind::PaperLiked,
        "PaperUnliked" => PaperProofEventKind::PaperUnliked,
        "ProposalCreated" => PaperProofEventKind::ProposalCreated,
        "ProposalVoted" => PaperProofEventKind::ProposalVoted,
        "ProposalFinalized" => PaperProofEventKind::ProposalFinalized,
        "ProposalExecuted" => PaperProofEventKind::ProposalExecuted,
        "ProposalExpired" => PaperProofEventKind::ProposalExpired,
        "VoteClaimed" => PaperProofEventKind::VoteClaimed,
        "GovernanceConfigMigrated" => PaperProofEventKind::GovernanceConfigMigrated,
        "ProposalMigrated" => PaperProofEventKind::ProposalMigrated,
        "ProposalCreationPausedChanged" => PaperProofEventKind::ProposalCreationPausedChanged,
        "ProposerThresholdChanged" => PaperProofEventKind::ProposerThresholdChanged,
        "ProposalDurationChanged" => PaperProofEventKind::ProposalDurationChanged,
        "GovernanceActionStatusChanged" => PaperProofEventKind::GovernanceActionStatusChanged,
        "ArtifactStatusChanged" => PaperProofEventKind::ArtifactStatusChanged,
        "ProtocolPausedChanged" => PaperProofEventKind::ProtocolPausedChanged,
        "FeeRecipientChanged" => PaperProofEventKind::FeeRecipientChanged,
        "GovernanceAuthorityChanged" => PaperProofEventKind::GovernanceAuthorityChanged,
        "CommentsFeeLevelChanged" => PaperProofEventKind::CommentsFeeLevelChanged,
        "ArtifactFeeLevelChanged" => PaperProofEventKind::ArtifactFeeLevelChanged,
        "UpgradeAuthorityChanged" => PaperProofEventKind::UpgradeAuthorityChanged,
        "FeeCollected" => PaperProofEventKind::FeeCollected,
        "DirectAuthorityModeChanged" => PaperProofEventKind::DirectAuthorityModeChanged,
        "OperatorNominated" => PaperProofEventKind::OperatorNominated,
        "OperatorTransferCancelled" => PaperProofEventKind::OperatorTransferCancelled,
        "ManagedUpgradeCapRegistered" => PaperProofEventKind::ManagedUpgradeCapRegistered,
        "ManagedUpgradeAuthorized" => PaperProofEventKind::ManagedUpgradeAuthorized,
        "ManagedUpgradeCommitted" => PaperProofEventKind::ManagedUpgradeCommitted,
        "GovernanceVaultMigrated" => PaperProofEventKind::GovernanceVaultMigrated,
        "OwnerTransferred" => PaperProofEventKind::OwnerTransferred,
        _ => PaperProofEventKind::Unknown,
    }
}

#[cfg(feature = "postgres")]
fn indexed_event_from_postgres_row(
    row: &tokio_postgres::Row,
) -> paperproof_sdk_rs::Result<IndexedPaperProofEvent> {
    let event_key: String = row.get(0);
    let checkpoint: Option<i64> = row.get(1);
    let transaction_digest: Option<String> = row.get(2);
    let event_seq: Option<i64> = row.get(3);
    let package_id: String = row.get(4);
    let module: String = row.get(5);
    let event_type: String = row.get(6);
    let kind: String = row.get(7);
    let sender: Option<String> = row.get(8);
    let timestamp_ms: Option<i64> = row.get(9);
    let parsed_json: Value = row.get(10);
    let kind = parse_kind(&kind);
    let event = paperproof_sdk_rs::events::SuiEventEnvelope {
        id: Some(serde_json::json!({
            "eventKey": event_key,
            "txDigest": transaction_digest,
            "eventSeq": event_seq,
            "checkpoint": checkpoint
        })),
        package_id: package_id.clone(),
        transaction_module: module.clone(),
        sender: sender.unwrap_or_default(),
        event_type: event_type.clone(),
        parsed_json,
        bcs: None,
        timestamp_ms: timestamp_ms.map(|value| value.to_string()),
    };
    Ok(IndexedPaperProofEvent {
        id: paperproof_sdk_rs::EventId {
            checkpoint: checkpoint.and_then(|value| u64::try_from(value).ok()),
            transaction_digest,
            event_seq: event_seq.and_then(|value| u64::try_from(value).ok()),
            package_id,
            module,
            event_type,
        },
        verification: paperproof_sdk_rs::events_trust::verification_report_from_canonical_check(
            &event,
            &paperproof_sdk_rs::MAINNET_DEPLOYMENT,
            paperproof_sdk_rs::EventTrustLevel::Canonical,
        ),
        trust: paperproof_sdk_rs::events_trust::EventTrustResult {
            trusted: true,
            reason: None,
            status: paperproof_sdk_rs::events_trust::EventVerificationStatus::Canonical,
            issues: vec![],
        },
        kind,
        event,
    })
}

#[cfg(feature = "sqlite")]
fn clear_normalized_sqlite(conn: &rusqlite::Connection) -> paperproof_sdk_rs::Result<()> {
    conn.execute_batch(
        "delete from paperproof_content_refs;
         delete from paperproof_content_cache;
         delete from domain_activity;
         delete from domain_votes;
         delete from domain_governance_proposals;
         delete from domain_comments;
         delete from domain_versions;
         delete from domain_artifacts;
         delete from domain_airdrop_scores;",
    )
    .map_err(sqlite_err("clear normalized tables"))
}

#[cfg(feature = "postgres")]
async fn clear_normalized_postgres(
    client: &tokio_postgres::Client,
) -> paperproof_sdk_rs::Result<()> {
    client
        .batch_execute(
            "delete from paperproof_content_refs;
             delete from paperproof_content_cache;
             delete from domain_activity;
             delete from domain_votes;
             delete from domain_governance_proposals;
             delete from domain_comments;
             delete from domain_versions;
             delete from domain_artifacts;
             delete from domain_airdrop_scores;",
        )
        .await
        .map_err(postgres_err("clear postgres normalized tables"))
}

#[cfg(feature = "sqlite")]
fn insert_version(
    conn: &rusqlite::Connection,
    fields: &Value,
    raw: &str,
    created_at: &Option<String>,
    series_id: &str,
    version_id: &str,
) -> paperproof_sdk_rs::Result<()> {
    conn.execute(
        "insert into domain_versions (
            version_id, series_id, artifact_type, version, content_hash,
            walrus_blob_id, content_type, created_at, raw_json
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        on conflict(version_id) do update set
            series_id = excluded.series_id,
            artifact_type = coalesce(excluded.artifact_type, domain_versions.artifact_type),
            version = coalesce(excluded.version, domain_versions.version),
            content_hash = coalesce(excluded.content_hash, domain_versions.content_hash),
            walrus_blob_id = coalesce(excluded.walrus_blob_id, domain_versions.walrus_blob_id),
            content_type = coalesce(excluded.content_type, domain_versions.content_type),
            raw_json = excluded.raw_json",
        rusqlite::params![
            version_id,
            series_id,
            opt_u64_to_i64(u64_field(fields, "artifact_type"))?,
            opt_u64_to_i64(u64_field(fields, "version"))?,
            str_field(fields, "content_hash"),
            str_field(fields, "walrus_blob_id").or_else(|| str_field(fields, "blob_id")),
            str_field(fields, "content_type"),
            created_at,
            raw,
        ],
    )
    .map_err(sqlite_err("upsert version"))?;
    if let Some(blob_id) =
        str_field(fields, "walrus_blob_id").or_else(|| str_field(fields, "blob_id"))
    {
        let expected_sha256_hex = str_field(fields, "content_hash")
            .map(|value| value.trim_start_matches("sha256:").to_string());
        conn.execute(
            "insert into paperproof_content_refs (
                source_event_key, artifact_id, version_id, blob_id, expected_sha256_hex,
                content_type, status, details_json
            ) values (?1, ?2, ?3, ?4, ?5, ?6, 'pending', null)
            on conflict(source_event_key) do update set
                artifact_id = excluded.artifact_id,
                version_id = excluded.version_id,
                blob_id = excluded.blob_id,
                expected_sha256_hex = excluded.expected_sha256_hex,
                content_type = excluded.content_type,
                updated_at = current_timestamp",
            rusqlite::params![
                format!("{series_id}:{version_id}:{blob_id}"),
                series_id,
                version_id,
                blob_id,
                expected_sha256_hex,
                str_field(fields, "content_type"),
            ],
        )
        .map_err(sqlite_err("upsert content ref"))?;
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
fn insert_activity(
    conn: &rusqlite::Connection,
    event: &IndexedPaperProofEvent,
    event_key: &str,
    created_at: &Option<String>,
) -> paperproof_sdk_rs::Result<()> {
    let fields = &event.event.parsed_json;
    conn.execute(
        "insert or ignore into domain_activity (
            event_key, kind, actor, series_id, proposal_id, tree_id, created_at, raw_json
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            event_key,
            format!("{:?}", event.kind),
            actor_for_event(event),
            activity_series_id(conn, fields)?,
            opt_u64_to_i64(u64_field(fields, "proposal_id"))?,
            str_field(fields, "tree_id"),
            created_at,
            serde_json::to_string(fields)?,
        ],
    )
    .map_err(sqlite_err("insert activity"))?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn activity_series_id(
    conn: &rusqlite::Connection,
    fields: &Value,
) -> paperproof_sdk_rs::Result<Option<String>> {
    if let Some(series_id) = str_field(fields, "series_id") {
        return Ok(Some(series_id));
    }
    let Some(tree_id) = str_field(fields, "tree_id") else {
        return Ok(None);
    };
    series_for_tree(conn, &tree_id)
}

#[cfg(feature = "sqlite")]
fn bump_score(
    conn: &rusqlite::Connection,
    address: &str,
    column: &str,
    weight: u64,
    reason: &str,
) -> paperproof_sdk_rs::Result<()> {
    if address.trim().is_empty() {
        return Ok(());
    }
    let score = u64_to_i64(weight)?;
    let sql = format!(
        "insert into domain_airdrop_scores (address, {column}, score, reasons_json)
         values (?1, 1, ?2, json_array(?3))
         on conflict(address) do update set
            {column} = {column} + 1,
            score = score + ?2,
            reasons_json = json_insert(reasons_json, '$[#]', ?3),
            updated_at = current_timestamp"
    );
    conn.execute(&sql, rusqlite::params![address, score, reason])
        .map_err(sqlite_err("bump airdrop score"))?;
    Ok(())
}

#[cfg(feature = "sqlite")]
fn series_for_tree(
    conn: &rusqlite::Connection,
    tree_id: &str,
) -> paperproof_sdk_rs::Result<Option<String>> {
    if tree_id.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn
        .prepare("select series_id from domain_artifacts where comments_tree_id = ?1")
        .map_err(sqlite_err("prepare tree lookup"))?;
    let mut rows = stmt
        .query_map([tree_id], |row| row.get::<_, String>(0))
        .map_err(sqlite_err("query tree lookup"))?;
    rows.next()
        .transpose()
        .map_err(sqlite_err("read tree lookup"))
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn actor_for_event(event: &IndexedPaperProofEvent) -> Option<String> {
    let fields = &event.event.parsed_json;
    str_field(fields, "author")
        .or_else(|| str_field(fields, "commenter"))
        .or_else(|| str_field(fields, "voter"))
        .or_else(|| str_field(fields, "proposer"))
        .or_else(|| str_field(fields, "owner"))
        .or_else(|| str_field(fields, "publisher"))
        .or_else(|| Some(event.event.sender.clone()))
}

#[cfg(feature = "postgres")]
async fn apply_event_postgres(
    client: &tokio_postgres::Client,
    event: &IndexedPaperProofEvent,
) -> paperproof_sdk_rs::Result<()> {
    let fields = &event.event.parsed_json;
    let raw = fields.clone();
    let event_key = event.id.key();
    let created_at = event
        .event
        .timestamp_ms
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok());
    match event.kind {
        PaperProofEventKind::ArtifactPublished => {
            let Some(series_id) = str_field(fields, "series_id") else {
                return Ok(());
            };
            let version_id = str_field(fields, "version_id");
            client
                .execute(
                    "insert into domain_artifacts (
                        series_id, artifact_code, artifact_type, owner, latest_version_id,
                        comments_tree_id, likes_book_id, title, status, published_at, updated_at, raw_json
                    ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000), now(), $11)
                    on conflict(series_id) do update set
                        artifact_code = coalesce(excluded.artifact_code, domain_artifacts.artifact_code),
                        artifact_type = coalesce(excluded.artifact_type, domain_artifacts.artifact_type),
                        owner = coalesce(excluded.owner, domain_artifacts.owner),
                        latest_version_id = coalesce(excluded.latest_version_id, domain_artifacts.latest_version_id),
                        comments_tree_id = coalesce(excluded.comments_tree_id, domain_artifacts.comments_tree_id),
                        likes_book_id = coalesce(excluded.likes_book_id, domain_artifacts.likes_book_id),
                        title = coalesce(excluded.title, domain_artifacts.title),
                        updated_at = now(),
                        raw_json = excluded.raw_json",
                    &[
                        &series_id,
                        &str_field(fields, "artifact_code"),
                        &opt_u64_to_i64_pg(u64_field(fields, "artifact_type"))?,
                        &str_field(fields, "owner")
                            .or_else(|| str_field(fields, "publisher"))
                            .or_else(|| Some(event.event.sender.clone())),
                        &version_id,
                        &str_field(fields, "comments_tree_id"),
                        &str_field(fields, "likes_book_id"),
                        &str_field(fields, "title").or_else(|| str_field(fields, "artifact_code")),
                        &opt_u64_to_i64_pg(u64_field(fields, "status"))?,
                        &created_at,
                        &raw,
                    ],
                )
                .await
                .map_err(postgres_err("postgres upsert artifact"))?;
            if let Some(version_id) = version_id {
                insert_version_postgres(client, fields, &raw, created_at, &series_id, &version_id)
                    .await?;
            }
            bump_score_postgres(
                client,
                &event.event.sender,
                "published_artifacts",
                10,
                "published artifact",
            )
            .await?;
        }
        PaperProofEventKind::ArtifactVersionAdded => {
            let Some(series_id) = str_field(fields, "series_id") else {
                return Ok(());
            };
            let Some(version_id) =
                str_field(fields, "version_id").or_else(|| str_field(fields, "new_version_id"))
            else {
                return Ok(());
            };
            insert_version_postgres(client, fields, &raw, created_at, &series_id, &version_id)
                .await?;
            client
                .execute(
                    "update domain_artifacts set latest_version_id = $1, updated_at = now() where series_id = $2",
                    &[&version_id, &series_id],
                )
                .await
                .map_err(postgres_err("postgres update latest version"))?;
            bump_score_postgres(
                client,
                &event.event.sender,
                "versions_added",
                3,
                "added version",
            )
            .await?;
        }
        PaperProofEventKind::OwnerTransferred => {
            if let Some(series_id) = str_field(fields, "series_id") {
                client
                    .execute(
                        "update domain_artifacts set owner = $1, updated_at = now() where series_id = $2",
                        &[&str_field(fields, "new_owner"), &series_id],
                    )
                    .await
                    .map_err(postgres_err("postgres update owner"))?;
            }
        }
        PaperProofEventKind::CommentAdded => {
            let Some(tree_id) = str_field(fields, "tree_id") else {
                return Ok(());
            };
            let Some(comment_id) = u64_field(fields, "comment_id") else {
                return Ok(());
            };
            let series_id = series_for_tree_postgres(client, &tree_id)
                .await?
                .or_else(|| str_field(fields, "series_id"))
                .or_else(|| str_field(fields, "target_series_id"));
            client
                .execute(
                    "insert into domain_comments (
                        tree_id, comment_id, parent_comment_id, series_id, author, content_mode,
                        status, created_at, updated_at, raw_json
                    ) values ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000), now(), $9)
                    on conflict(tree_id, comment_id) do update set
                        parent_comment_id = excluded.parent_comment_id,
                        series_id = coalesce(excluded.series_id, domain_comments.series_id),
                        author = coalesce(excluded.author, domain_comments.author),
                        content_mode = coalesce(excluded.content_mode, domain_comments.content_mode),
                        updated_at = now(),
                        raw_json = excluded.raw_json",
                    &[
                        &tree_id,
                        &u64_to_i64_pg(comment_id)?,
                        &opt_u64_to_i64_pg(u64_field(fields, "parent_comment_id"))?,
                        &series_id,
                        &str_field(fields, "author")
                            .or_else(|| str_field(fields, "commenter"))
                            .or_else(|| Some(event.event.sender.clone())),
                        &opt_u64_to_i64_pg(u64_field(fields, "content_mode"))?,
                        &opt_u64_to_i64_pg(u64_field(fields, "status"))?,
                        &created_at,
                        &raw,
                    ],
                )
                .await
                .map_err(postgres_err("postgres upsert comment"))?;
            bump_score_postgres(client, &event.event.sender, "comments", 1, "commented").await?;
        }
        PaperProofEventKind::CommentStatusChanged => {
            if let (Some(tree_id), Some(comment_id)) = (
                str_field(fields, "tree_id"),
                u64_field(fields, "comment_id"),
            ) {
                client
                    .execute(
                        "update domain_comments set status = $1, updated_at = now(), raw_json = $2 where tree_id = $3 and comment_id = $4",
                        &[
                            &opt_u64_to_i64_pg(u64_field(fields, "new_status"))?,
                            &raw,
                            &tree_id,
                            &u64_to_i64_pg(comment_id)?,
                        ],
                    )
                    .await
                    .map_err(postgres_err("postgres update comment status"))?;
            }
        }
        PaperProofEventKind::PaperLiked => {
            bump_score_postgres(client, &event.event.sender, "likes", 1, "liked artifact").await?;
        }
        PaperProofEventKind::ProposalCreated => {
            let Some(proposal_id) = u64_field(fields, "proposal_id") else {
                return Ok(());
            };
            client
                .execute(
                    "insert into domain_governance_proposals (
                        proposal_id, proposal_object_id, proposer, title, action_type, proposal_type,
                        status, yes_votes, no_votes, created_at, updated_at, raw_json
                    ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000), now(), $11)
                    on conflict(proposal_id) do update set
                        proposal_object_id = coalesce(excluded.proposal_object_id, domain_governance_proposals.proposal_object_id),
                        proposer = coalesce(excluded.proposer, domain_governance_proposals.proposer),
                        title = coalesce(excluded.title, domain_governance_proposals.title),
                        action_type = coalesce(excluded.action_type, domain_governance_proposals.action_type),
                        proposal_type = coalesce(excluded.proposal_type, domain_governance_proposals.proposal_type),
                        updated_at = now(),
                        raw_json = excluded.raw_json",
                    &[
                        &u64_to_i64_pg(proposal_id)?,
                        &str_field(fields, "proposal_object_id"),
                        &str_field(fields, "proposer").or_else(|| Some(event.event.sender.clone())),
                        &str_field(fields, "title"),
                        &opt_u64_to_i64_pg(u64_field(fields, "action_type"))?,
                        &opt_u64_to_i64_pg(u64_field(fields, "proposal_type"))?,
                        &opt_u64_to_i64_pg(u64_field(fields, "status"))?,
                        &str_field(fields, "yes_votes"),
                        &str_field(fields, "no_votes"),
                        &created_at,
                        &raw,
                    ],
                )
                .await
                .map_err(postgres_err("postgres upsert proposal"))?;
        }
        PaperProofEventKind::ProposalVoted => {
            let Some(proposal_id) = u64_field(fields, "proposal_id") else {
                return Ok(());
            };
            let voter = str_field(fields, "voter").unwrap_or_else(|| event.event.sender.clone());
            client
                .execute(
                    "insert into domain_votes (
                        proposal_id, voter, side, voting_power, claimed, created_at, updated_at, raw_json
                    ) values ($1, $2, $3, $4, false, to_timestamp($5::double precision / 1000), now(), $6)
                    on conflict(proposal_id, voter) do update set
                        side = excluded.side,
                        voting_power = excluded.voting_power,
                        updated_at = now(),
                        raw_json = excluded.raw_json",
                    &[
                        &u64_to_i64_pg(proposal_id)?,
                        &voter,
                        &opt_u64_to_i64_pg(u64_field(fields, "side"))?,
                        &str_field(fields, "voting_power")
                            .or_else(|| u64_field(fields, "voting_power").map(|v| v.to_string())),
                        &created_at,
                        &raw,
                    ],
                )
                .await
                .map_err(postgres_err("postgres upsert vote"))?;
            bump_score_postgres(client, &event.event.sender, "votes", 5, "voted").await?;
        }
        PaperProofEventKind::ProposalFinalized | PaperProofEventKind::ProposalExpired => {
            if let Some(proposal_id) = u64_field(fields, "proposal_id") {
                client
                    .execute(
                        "update domain_governance_proposals set status = $1, yes_votes = coalesce($2, yes_votes), no_votes = coalesce($3, no_votes), updated_at = now(), raw_json = $4 where proposal_id = $5",
                        &[
                            &opt_u64_to_i64_pg(u64_field(fields, "status"))?,
                            &str_field(fields, "yes_votes"),
                            &str_field(fields, "no_votes"),
                            &raw,
                            &u64_to_i64_pg(proposal_id)?,
                        ],
                    )
                    .await
                    .map_err(postgres_err("postgres update proposal status"))?;
            }
        }
        PaperProofEventKind::VoteClaimed => {
            if let (Some(proposal_id), Some(voter)) =
                (u64_field(fields, "proposal_id"), str_field(fields, "voter"))
            {
                client
                    .execute(
                        "update domain_votes set claimed = true, updated_at = now(), raw_json = $1 where proposal_id = $2 and lower(voter) = lower($3)",
                        &[&raw, &u64_to_i64_pg(proposal_id)?, &voter],
                    )
                    .await
                    .map_err(postgres_err("postgres mark claimed"))?;
            }
        }
        _ => {}
    }
    insert_activity_postgres(client, event, &event_key, created_at).await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn insert_version_postgres(
    client: &tokio_postgres::Client,
    fields: &Value,
    raw: &Value,
    created_at: Option<i64>,
    series_id: &str,
    version_id: &str,
) -> paperproof_sdk_rs::Result<()> {
    client
        .execute(
            "insert into domain_versions (
                version_id, series_id, artifact_type, version, content_hash,
                walrus_blob_id, content_type, created_at, raw_json
            ) values ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000), $9)
            on conflict(version_id) do update set
                series_id = excluded.series_id,
                artifact_type = coalesce(excluded.artifact_type, domain_versions.artifact_type),
                version = coalesce(excluded.version, domain_versions.version),
                content_hash = coalesce(excluded.content_hash, domain_versions.content_hash),
                walrus_blob_id = coalesce(excluded.walrus_blob_id, domain_versions.walrus_blob_id),
                content_type = coalesce(excluded.content_type, domain_versions.content_type),
                raw_json = excluded.raw_json",
            &[
                &version_id,
                &series_id,
                &opt_u64_to_i64_pg(u64_field(fields, "artifact_type"))?,
                &opt_u64_to_i64_pg(u64_field(fields, "version"))?,
                &str_field(fields, "content_hash"),
                &str_field(fields, "walrus_blob_id").or_else(|| str_field(fields, "blob_id")),
                &str_field(fields, "content_type"),
                &created_at,
                raw,
            ],
        )
        .await
        .map_err(postgres_err("postgres upsert version"))?;
    if let Some(blob_id) =
        str_field(fields, "walrus_blob_id").or_else(|| str_field(fields, "blob_id"))
    {
        let expected_sha256_hex = str_field(fields, "content_hash")
            .map(|value| value.trim_start_matches("sha256:").to_string());
        client
            .execute(
                "insert into paperproof_content_refs (
                    source_event_key, artifact_id, version_id, blob_id, expected_sha256_hex,
                    content_type, status, details_json
                ) values ($1, $2, $3, $4, $5, $6, 'pending', null)
                on conflict(source_event_key) do update set
                    artifact_id = excluded.artifact_id,
                    version_id = excluded.version_id,
                    blob_id = excluded.blob_id,
                    expected_sha256_hex = excluded.expected_sha256_hex,
                    content_type = excluded.content_type,
                    updated_at = now()",
                &[
                    &format!("{series_id}:{version_id}:{blob_id}"),
                    &series_id,
                    &version_id,
                    &blob_id,
                    &expected_sha256_hex,
                    &str_field(fields, "content_type"),
                ],
            )
            .await
            .map_err(postgres_err("postgres upsert content ref"))?;
    }
    Ok(())
}

#[cfg(feature = "postgres")]
async fn insert_activity_postgres(
    client: &tokio_postgres::Client,
    event: &IndexedPaperProofEvent,
    event_key: &str,
    created_at: Option<i64>,
) -> paperproof_sdk_rs::Result<()> {
    let fields = &event.event.parsed_json;
    client
        .execute(
            "insert into domain_activity (
                event_key, kind, actor, series_id, proposal_id, tree_id, created_at, raw_json
            ) values ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000), $8)
            on conflict(event_key) do nothing",
            &[
                &event_key,
                &format!("{:?}", event.kind),
                &actor_for_event(event),
                &activity_series_id_postgres(client, fields).await?,
                &opt_u64_to_i64_pg(u64_field(fields, "proposal_id"))?,
                &str_field(fields, "tree_id"),
                &created_at,
                fields,
            ],
        )
        .await
        .map_err(postgres_err("postgres insert activity"))?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn activity_series_id_postgres(
    client: &tokio_postgres::Client,
    fields: &Value,
) -> paperproof_sdk_rs::Result<Option<String>> {
    if let Some(series_id) = str_field(fields, "series_id") {
        return Ok(Some(series_id));
    }
    let Some(tree_id) = str_field(fields, "tree_id") else {
        return Ok(None);
    };
    series_for_tree_postgres(client, &tree_id).await
}

#[cfg(feature = "postgres")]
async fn series_for_tree_postgres(
    client: &tokio_postgres::Client,
    tree_id: &str,
) -> paperproof_sdk_rs::Result<Option<String>> {
    let row = client
        .query_opt(
            "select series_id from domain_artifacts where comments_tree_id = $1",
            &[&tree_id],
        )
        .await
        .map_err(postgres_err("postgres tree lookup"))?;
    Ok(row.map(|row| row.get(0)))
}

#[cfg(feature = "postgres")]
async fn bump_score_postgres(
    client: &tokio_postgres::Client,
    address: &str,
    column: &str,
    weight: u64,
    reason: &str,
) -> paperproof_sdk_rs::Result<()> {
    if address.trim().is_empty() {
        return Ok(());
    }
    let score = u64_to_i64_pg(weight)?;
    let sql = format!(
        "insert into domain_airdrop_scores (address, {column}, score, reasons_json)
         values ($1, 1, $2, jsonb_build_array($3::text))
         on conflict(address) do update set
            {column} = domain_airdrop_scores.{column} + 1,
            score = domain_airdrop_scores.score + $2,
            reasons_json = domain_airdrop_scores.reasons_json || jsonb_build_array($3::text),
            updated_at = now()"
    );
    client
        .execute(&sql, &[&address, &score, &reason])
        .await
        .map_err(postgres_err("postgres bump airdrop score"))?;
    Ok(())
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn str_field(fields: &Value, key: &str) -> Option<String> {
    fields
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn u64_field(fields: &Value, key: &str) -> Option<u64> {
    fields
        .get(key)
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}

#[cfg(feature = "sqlite")]
fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRecord> {
    Ok(ArtifactRecord {
        series_id: row.get(0)?,
        artifact_code: row.get(1)?,
        artifact_type: opt_i64_to_u64(row.get(2)?)?,
        owner: row.get(3)?,
        latest_version_id: row.get(4)?,
        comments_tree_id: row.get(5)?,
        likes_book_id: row.get(6)?,
        title: row.get(7)?,
        status: opt_i64_to_u64(row.get(8)?)?,
        published_at: row.get(9)?,
        updated_at: row.get(10)?,
        raw_json: json_from_row(row, 11)?,
    })
}

#[cfg(feature = "sqlite")]
fn version_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VersionRecord> {
    Ok(VersionRecord {
        version_id: row.get(0)?,
        series_id: row.get(1)?,
        artifact_type: opt_i64_to_u64(row.get(2)?)?,
        version: opt_i64_to_u64(row.get(3)?)?,
        content_hash: row.get(4)?,
        walrus_blob_id: row.get(5)?,
        content_type: row.get(6)?,
        created_at: row.get(7)?,
        raw_json: json_from_row(row, 8)?,
    })
}

#[cfg(feature = "sqlite")]
fn comment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommentRecord> {
    Ok(CommentRecord {
        tree_id: row.get(0)?,
        comment_id: i64_to_u64(row.get(1)?)?,
        parent_comment_id: opt_i64_to_u64(row.get(2)?)?,
        series_id: row.get(3)?,
        author: row.get(4)?,
        content_mode: opt_i64_to_u64(row.get(5)?)?,
        status: opt_i64_to_u64(row.get(6)?)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        raw_json: json_from_row(row, 9)?,
    })
}

#[cfg(feature = "sqlite")]
fn proposal_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GovernanceProposalRecord> {
    Ok(GovernanceProposalRecord {
        proposal_id: i64_to_u64(row.get(0)?)?,
        proposal_object_id: row.get(1)?,
        proposer: row.get(2)?,
        title: row.get(3)?,
        action_type: opt_i64_to_u64(row.get(4)?)?,
        proposal_type: opt_i64_to_u64(row.get(5)?)?,
        status: opt_i64_to_u64(row.get(6)?)?,
        yes_votes: row.get(7)?,
        no_votes: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        raw_json: json_from_row(row, 11)?,
    })
}

#[cfg(feature = "sqlite")]
fn vote_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GovernanceVoteRecord> {
    Ok(GovernanceVoteRecord {
        proposal_id: i64_to_u64(row.get(0)?)?,
        voter: row.get(1)?,
        side: opt_i64_to_u64(row.get(2)?)?,
        voting_power: row.get(3)?,
        claimed: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        raw_json: json_from_row(row, 7)?,
    })
}

#[cfg(feature = "sqlite")]
fn activity_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActivityRecord> {
    Ok(ActivityRecord {
        event_key: row.get(0)?,
        kind: row.get(1)?,
        actor: row.get(2)?,
        series_id: row.get(3)?,
        proposal_id: opt_i64_to_u64(row.get(4)?)?,
        tree_id: row.get(5)?,
        created_at: row.get(6)?,
        raw_json: json_from_row(row, 7)?,
    })
}

#[cfg(feature = "postgres")]
fn pg_artifact_from_row(row: &tokio_postgres::Row) -> paperproof_sdk_rs::Result<ArtifactRecord> {
    Ok(ArtifactRecord {
        series_id: row.get(0),
        artifact_code: row.get(1),
        artifact_type: opt_i64_to_u64_pg(row.get(2))?,
        owner: row.get(3),
        latest_version_id: row.get(4),
        comments_tree_id: row.get(5),
        likes_book_id: row.get(6),
        title: row.get(7),
        status: opt_i64_to_u64_pg(row.get(8))?,
        published_at: row.get(9),
        updated_at: row.get(10),
        raw_json: row.get(11),
    })
}

#[cfg(feature = "postgres")]
fn pg_version_from_row(row: &tokio_postgres::Row) -> paperproof_sdk_rs::Result<VersionRecord> {
    Ok(VersionRecord {
        version_id: row.get(0),
        series_id: row.get(1),
        artifact_type: opt_i64_to_u64_pg(row.get(2))?,
        version: opt_i64_to_u64_pg(row.get(3))?,
        content_hash: row.get(4),
        walrus_blob_id: row.get(5),
        content_type: row.get(6),
        created_at: row.get(7),
        raw_json: row.get(8),
    })
}

#[cfg(feature = "postgres")]
fn pg_comment_from_row(row: &tokio_postgres::Row) -> paperproof_sdk_rs::Result<CommentRecord> {
    Ok(CommentRecord {
        tree_id: row.get(0),
        comment_id: i64_to_u64_pg(row.get(1))?,
        parent_comment_id: opt_i64_to_u64_pg(row.get(2))?,
        series_id: row.get(3),
        author: row.get(4),
        content_mode: opt_i64_to_u64_pg(row.get(5))?,
        status: opt_i64_to_u64_pg(row.get(6))?,
        created_at: row.get(7),
        updated_at: row.get(8),
        raw_json: row.get(9),
    })
}

#[cfg(feature = "postgres")]
fn pg_proposal_from_row(
    row: &tokio_postgres::Row,
) -> paperproof_sdk_rs::Result<GovernanceProposalRecord> {
    Ok(GovernanceProposalRecord {
        proposal_id: i64_to_u64_pg(row.get(0))?,
        proposal_object_id: row.get(1),
        proposer: row.get(2),
        title: row.get(3),
        action_type: opt_i64_to_u64_pg(row.get(4))?,
        proposal_type: opt_i64_to_u64_pg(row.get(5))?,
        status: opt_i64_to_u64_pg(row.get(6))?,
        yes_votes: row.get(7),
        no_votes: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
        raw_json: row.get(11),
    })
}

#[cfg(feature = "postgres")]
fn pg_vote_from_row(row: &tokio_postgres::Row) -> paperproof_sdk_rs::Result<GovernanceVoteRecord> {
    Ok(GovernanceVoteRecord {
        proposal_id: i64_to_u64_pg(row.get(0))?,
        voter: row.get(1),
        side: opt_i64_to_u64_pg(row.get(2))?,
        voting_power: row.get(3),
        claimed: row.get(4),
        created_at: row.get(5),
        updated_at: row.get(6),
        raw_json: row.get(7),
    })
}

#[cfg(feature = "postgres")]
fn pg_activity_from_row(row: &tokio_postgres::Row) -> paperproof_sdk_rs::Result<ActivityRecord> {
    Ok(ActivityRecord {
        event_key: row.get(0),
        kind: row.get(1),
        actor: row.get(2),
        series_id: row.get(3),
        proposal_id: opt_i64_to_u64_pg(row.get(4))?,
        tree_id: row.get(5),
        created_at: row.get(6),
        raw_json: row.get(7),
    })
}

#[cfg(feature = "postgres")]
fn pg_airdrop_from_row(row: &tokio_postgres::Row) -> paperproof_sdk_rs::Result<AirdropRow> {
    let reasons: Value = row.get(7);
    Ok(AirdropRow {
        address: row.get(0),
        published_artifacts: i64_to_u64_pg(row.get(1))?,
        versions_added: i64_to_u64_pg(row.get(2))?,
        comments: i64_to_u64_pg(row.get(3))?,
        votes: i64_to_u64_pg(row.get(4))?,
        likes: i64_to_u64_pg(row.get(5))?,
        score: i64_to_u64_pg(row.get(6))?,
        reasons: serde_json::from_value(reasons).unwrap_or_default(),
    })
}

#[cfg(feature = "sqlite")]
fn json_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let text: String = row.get(index)?;
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

#[cfg(feature = "sqlite")]
fn count(conn: &rusqlite::Connection, table: &str) -> paperproof_sdk_rs::Result<u64> {
    scalar_i64(conn, &format!("select count(*) from {table}")).map(|value| value as u64)
}

#[cfg(feature = "sqlite")]
fn scalar_i64(conn: &rusqlite::Connection, sql: &str) -> paperproof_sdk_rs::Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
        .map_err(sqlite_err("read scalar"))
}

#[cfg(feature = "postgres")]
async fn pg_count(client: &tokio_postgres::Client, table: &str) -> paperproof_sdk_rs::Result<u64> {
    pg_scalar_i64(client, &format!("select count(*) from {table}"))
        .await
        .and_then(i64_to_u64_pg)
}

#[cfg(feature = "postgres")]
async fn pg_scalar_i64(
    client: &tokio_postgres::Client,
    sql: &str,
) -> paperproof_sdk_rs::Result<i64> {
    client
        .query_one(sql, &[])
        .await
        .map_err(postgres_err("postgres read scalar"))
        .map(|row| row.get(0))
}

#[cfg(feature = "postgres")]
async fn pg_top_contributors(
    client: &tokio_postgres::Client,
    limit: u64,
) -> paperproof_sdk_rs::Result<Vec<crate::analytics::ContributorSummary>> {
    let rows = client
        .query(
            "select address, score, published_artifacts, comments, votes
             from domain_airdrop_scores
             order by score desc, address asc
             limit $1",
            &[&u64_to_i64_pg(limit)?],
        )
        .await
        .map_err(postgres_err("postgres top contributors"))?;
    rows.iter()
        .map(|row| {
            Ok(crate::analytics::ContributorSummary {
                address: row.get(0),
                score: i64_to_u64_pg(row.get(1))?,
                published_artifacts: i64_to_u64_pg(row.get(2))?,
                comments: i64_to_u64_pg(row.get(3))?,
                votes: i64_to_u64_pg(row.get(4))?,
            })
        })
        .collect()
}

#[cfg(feature = "postgres")]
async fn pg_artifact_type_summary(
    client: &tokio_postgres::Client,
) -> paperproof_sdk_rs::Result<Vec<crate::analytics::ArtifactTypeSummary>> {
    let rows = client
        .query(
            "select artifact_type, count(*)
             from domain_artifacts
             where artifact_type is not null
             group by artifact_type
             order by artifact_type asc",
            &[],
        )
        .await
        .map_err(postgres_err("postgres artifact type summary"))?;
    rows.iter()
        .map(|row| {
            Ok(crate::analytics::ArtifactTypeSummary {
                artifact_type: i64_to_u64_pg(row.get(0))?,
                count: i64_to_u64_pg(row.get(1))?,
            })
        })
        .collect()
}

#[cfg(feature = "sqlite")]
fn top_contributors(
    conn: &rusqlite::Connection,
    limit: u64,
) -> paperproof_sdk_rs::Result<Vec<crate::analytics::ContributorSummary>> {
    let mut stmt = conn
        .prepare("select address, score, published_artifacts, comments, votes from domain_airdrop_scores order by score desc, address asc limit ?1")
        .map_err(sqlite_err("prepare top contributors"))?;
    stmt.query_map([u64_to_i64(limit)?], |row| {
        Ok(crate::analytics::ContributorSummary {
            address: row.get(0)?,
            score: i64_to_u64(row.get(1)?)?,
            published_artifacts: i64_to_u64(row.get(2)?)?,
            comments: i64_to_u64(row.get(3)?)?,
            votes: i64_to_u64(row.get(4)?)?,
        })
    })
    .map_err(sqlite_err("query top contributors"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sqlite_err("read top contributors"))
}

#[cfg(feature = "sqlite")]
fn artifact_type_summary(
    conn: &rusqlite::Connection,
) -> paperproof_sdk_rs::Result<Vec<crate::analytics::ArtifactTypeSummary>> {
    let mut stmt = conn
        .prepare("select artifact_type, count(*) from domain_artifacts where artifact_type is not null group by artifact_type order by artifact_type asc")
        .map_err(sqlite_err("prepare artifact type summary"))?;
    stmt.query_map([], |row| {
        Ok(crate::analytics::ArtifactTypeSummary {
            artifact_type: i64_to_u64(row.get(0)?)?,
            count: i64_to_u64(row.get(1)?)?,
        })
    })
    .map_err(sqlite_err("query artifact type summary"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sqlite_err("read artifact type summary"))
}

#[cfg(feature = "sqlite")]
fn opt_u64_to_i64(value: Option<u64>) -> paperproof_sdk_rs::Result<Option<i64>> {
    value.map(u64_to_i64).transpose()
}

#[cfg(feature = "sqlite")]
fn u64_to_i64(value: u64) -> paperproof_sdk_rs::Result<i64> {
    i64::try_from(value).map_err(|_| PaperProofError::invalid_input("u64", "value exceeds i64"))
}

#[cfg(feature = "postgres")]
fn opt_u64_to_i64_pg(value: Option<u64>) -> paperproof_sdk_rs::Result<Option<i64>> {
    value.map(u64_to_i64_pg).transpose()
}

#[cfg(feature = "postgres")]
fn u64_to_i64_pg(value: u64) -> paperproof_sdk_rs::Result<i64> {
    i64::try_from(value).map_err(|_| PaperProofError::invalid_input("u64", "value exceeds i64"))
}

#[cfg(feature = "postgres")]
fn opt_i64_to_u64_pg(value: Option<i64>) -> paperproof_sdk_rs::Result<Option<u64>> {
    value.map(i64_to_u64_pg).transpose()
}

#[cfg(feature = "postgres")]
fn i64_to_u64_pg(value: i64) -> paperproof_sdk_rs::Result<u64> {
    u64::try_from(value)
        .map_err(|_| PaperProofError::invalid_input("postgres integer", "negative value"))
}

#[cfg(feature = "sqlite")]
fn opt_i64_to_u64(value: Option<i64>) -> rusqlite::Result<Option<u64>> {
    value.map(i64_to_u64).transpose()
}

#[cfg(feature = "sqlite")]
fn i64_to_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Integer, Box::new(err))
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_err(
    context: &'static str,
) -> impl Fn(rusqlite::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| PaperProofError::network(context, err.to_string())
}

#[cfg(feature = "postgres")]
fn postgres_err(
    context: &'static str,
) -> impl Fn(tokio_postgres::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| PaperProofError::network(context, err.to_string())
}

#[cfg(feature = "sqlite")]
fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(not(feature = "sqlite"))]
fn sqlite_required() -> paperproof_sdk_rs::PaperProofError {
    PaperProofError::invalid_input("sqlite", "normalized queries require --features sqlite")
}
