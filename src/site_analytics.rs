// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SiteAnalyticsConfig {
    pub enabled: bool,
    pub salt: Option<String>,
    pub admin_token: Option<String>,
}

impl SiteAnalyticsConfig {
    pub fn active(&self) -> bool {
        self.enabled
            && self
                .salt
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }

    pub fn admin_allowed(&self, token: Option<&str>) -> bool {
        let Some(expected) = self
            .admin_token
            .as_deref()
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        token.is_some_and(|value| value == expected)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteVisitRequest {
    pub visitor_id: Option<String>,
    pub path: Option<String>,
    pub referrer: Option<String>,
    pub timezone: Option<String>,
    pub language: Option<String>,
    pub screen: Option<String>,
    pub device_pixel_ratio: Option<f64>,
    pub platform: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteVisitResponse {
    pub recorded: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteWeeklySummary {
    pub week_start: String,
    pub visits: u64,
    pub unique_visitors: u64,
    pub unique_ips: u64,
    pub top_paths: Vec<SitePathSummary>,
    pub countries: Vec<SiteCountrySummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePathSummary {
    pub path: String,
    pub visits: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteCountrySummary {
    pub country: String,
    pub visits: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedVisit {
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub accept_language: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedVisit {
    occurred_at: String,
    visitor_id_hash: Option<String>,
    ip_hash: Option<String>,
    fallback_visitor_key: String,
    user_agent_hash: Option<String>,
    path: String,
    referrer: Option<String>,
    language: Option<String>,
    timezone: Option<String>,
    screen: Option<String>,
    device_pixel_ratio: Option<String>,
    platform: Option<String>,
    country: Option<String>,
    week_start: String,
}

pub async fn record_visit(
    sqlite_path: Option<&str>,
    postgres_url: Option<&str>,
    config: &SiteAnalyticsConfig,
    request: SiteVisitRequest,
    observed: ObservedVisit,
) -> paperproof_sdk_rs::Result<SiteVisitResponse> {
    if !config.enabled {
        return Ok(SiteVisitResponse {
            recorded: false,
            reason: Some("disabled".to_string()),
        });
    }
    if !config.active() {
        return Ok(SiteVisitResponse {
            recorded: false,
            reason: Some("missing-salt".to_string()),
        });
    }
    let Some(salt) = config.salt.as_deref() else {
        return Ok(SiteVisitResponse {
            recorded: false,
            reason: Some("missing-salt".to_string()),
        });
    };
    let visit = normalize_visit(salt, request, observed);
    if let Some(path) = sqlite_path {
        record_visit_sqlite(path, &visit)?;
        return Ok(SiteVisitResponse {
            recorded: true,
            reason: None,
        });
    }
    if let Some(url) = postgres_url {
        record_visit_postgres(url, &visit).await?;
        return Ok(SiteVisitResponse {
            recorded: true,
            reason: None,
        });
    }
    Ok(SiteVisitResponse {
        recorded: false,
        reason: Some("database-unavailable".to_string()),
    })
}

pub async fn weekly_summary(
    sqlite_path: Option<&str>,
    postgres_url: Option<&str>,
    week_start: Option<&str>,
) -> paperproof_sdk_rs::Result<SiteWeeklySummary> {
    let week_start = week_start
        .map(|value| clamp_text(value, 32))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(current_week_start_utc);
    if let Some(path) = sqlite_path {
        return weekly_summary_sqlite(path, &week_start);
    }
    if let Some(url) = postgres_url {
        return weekly_summary_postgres(url, &week_start).await;
    }
    Ok(SiteWeeklySummary {
        week_start,
        ..Default::default()
    })
}

fn normalize_visit(
    salt: &str,
    request: SiteVisitRequest,
    observed: ObservedVisit,
) -> NormalizedVisit {
    let client_ip = observed
        .client_ip
        .as_deref()
        .map(canonical_ip)
        .filter(|value| !value.is_empty());
    let user_agent = observed
        .user_agent
        .as_deref()
        .map(|value| clamp_text(value, 512))
        .filter(|value| !value.is_empty());
    let accept_language = observed
        .accept_language
        .as_deref()
        .map(|value| clamp_text(value, 128))
        .filter(|value| !value.is_empty());
    let language = request
        .language
        .or_else(|| accept_language.clone())
        .map(|value| clamp_text(&value, 128))
        .filter(|value| !value.is_empty());
    let path = request
        .path
        .map(|value| clamp_text(&value, 512))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/".to_string());
    let visitor_id_hash = request
        .visitor_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| hash_fields(salt, &[("kind", "visitor"), ("visitor_id", value)]));
    let ip_hash = client_ip
        .as_deref()
        .map(|value| hash_fields(salt, &[("kind", "ip"), ("ip", value)]));
    let user_agent_hash = user_agent
        .as_deref()
        .map(|value| hash_fields(salt, &[("kind", "user_agent"), ("user_agent", value)]));
    let fallback_visitor_key = hash_fields(
        salt,
        &[
            ("kind", "fallback_visitor"),
            ("ip", client_ip.as_deref().unwrap_or("unknown")),
            ("user_agent", user_agent.as_deref().unwrap_or("unknown")),
            (
                "accept_language",
                accept_language.as_deref().unwrap_or("unknown"),
            ),
        ],
    );
    NormalizedVisit {
        occurred_at: current_timestamp_utc_plus_8(),
        visitor_id_hash,
        ip_hash,
        fallback_visitor_key,
        user_agent_hash,
        path,
        referrer: request
            .referrer
            .map(|value| clamp_text(&value, 512))
            .filter(|value| !value.is_empty()),
        language,
        timezone: request
            .timezone
            .map(|value| clamp_text(&value, 128))
            .filter(|value| !value.is_empty()),
        screen: request
            .screen
            .map(|value| clamp_text(&value, 64))
            .filter(|value| !value.is_empty()),
        device_pixel_ratio: request
            .device_pixel_ratio
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| format!("{value:.3}")),
        platform: request
            .platform
            .map(|value| clamp_text(&value, 128))
            .filter(|value| !value.is_empty()),
        country: None,
        week_start: current_week_start_utc(),
    }
}

#[cfg(feature = "sqlite")]
fn record_visit_sqlite(db_path: &str, visit: &NormalizedVisit) -> paperproof_sdk_rs::Result<()> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open site analytics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure site analytics schema"))?;
    conn.execute(
        "insert into site_visit_events (
            occurred_at, week_start, visitor_id_hash, ip_hash, fallback_visitor_key, user_agent_hash,
            path, referrer, language, timezone, screen, device_pixel_ratio, platform, country
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            visit.occurred_at,
            visit.week_start,
            visit.visitor_id_hash,
            visit.ip_hash,
            visit.fallback_visitor_key,
            visit.user_agent_hash,
            visit.path,
            visit.referrer,
            visit.language,
            visit.timezone,
            visit.screen,
            visit.device_pixel_ratio,
            visit.platform,
            visit.country,
        ],
    )
    .map_err(sqlite_err("insert site visit"))?;
    Ok(())
}

#[cfg(not(feature = "sqlite"))]
fn record_visit_sqlite(_db_path: &str, _visit: &NormalizedVisit) -> paperproof_sdk_rs::Result<()> {
    Err(paperproof_sdk_rs::PaperProofError::invalid_input(
        "sqlite",
        "site analytics sqlite writes require --features sqlite",
    ))
}

#[cfg(feature = "postgres")]
async fn record_visit_postgres(
    connection_string: &str,
    visit: &NormalizedVisit,
) -> paperproof_sdk_rs::Result<()> {
    let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
        .await
        .map_err(postgres_err("connect site analytics postgres"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("paperproof site analytics postgres connection closed: {error}");
        }
    });
    client
        .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
        .await
        .map_err(postgres_err("ensure site analytics postgres schema"))?;
    client
        .execute(
            "insert into site_visit_events (
                occurred_at, week_start, visitor_id_hash, ip_hash, fallback_visitor_key, user_agent_hash,
                path, referrer, language, timezone, screen, device_pixel_ratio, platform, country
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
            &[
                &visit.occurred_at,
                &visit.week_start,
                &visit.visitor_id_hash,
                &visit.ip_hash,
                &visit.fallback_visitor_key,
                &visit.user_agent_hash,
                &visit.path,
                &visit.referrer,
                &visit.language,
                &visit.timezone,
                &visit.screen,
                &visit.device_pixel_ratio,
                &visit.platform,
                &visit.country,
            ],
        )
        .await
        .map_err(postgres_err("insert site visit postgres"))?;
    Ok(())
}

#[cfg(not(feature = "postgres"))]
async fn record_visit_postgres(
    _connection_string: &str,
    _visit: &NormalizedVisit,
) -> paperproof_sdk_rs::Result<()> {
    Err(paperproof_sdk_rs::PaperProofError::invalid_input(
        "postgres",
        "site analytics postgres writes require --features postgres",
    ))
}

#[cfg(feature = "sqlite")]
fn weekly_summary_sqlite(
    db_path: &str,
    week_start: &str,
) -> paperproof_sdk_rs::Result<SiteWeeklySummary> {
    let conn = rusqlite::Connection::open(db_path).map_err(sqlite_err("open site analytics db"))?;
    conn.execute_batch(crate::schema::SQLITE_REFERENCE_SCHEMA)
        .map_err(sqlite_err("ensure site analytics schema"))?;
    let visits = sqlite_scalar(
        &conn,
        "select count(*) from site_visit_events where week_start = ?1",
        week_start,
    )?;
    let unique_visitors = sqlite_scalar(
        &conn,
        "select count(distinct coalesce(visitor_id_hash, fallback_visitor_key)) from site_visit_events where week_start = ?1",
        week_start,
    )?;
    let unique_ips = sqlite_scalar(
        &conn,
        "select count(distinct ip_hash) from site_visit_events where week_start = ?1 and ip_hash is not null",
        week_start,
    )?;
    let top_paths = sqlite_top_paths(&conn, week_start)?;
    let countries = sqlite_countries(&conn, week_start)?;
    Ok(SiteWeeklySummary {
        week_start: week_start.to_string(),
        visits,
        unique_visitors,
        unique_ips,
        top_paths,
        countries,
    })
}

#[cfg(not(feature = "sqlite"))]
fn weekly_summary_sqlite(
    _db_path: &str,
    week_start: &str,
) -> paperproof_sdk_rs::Result<SiteWeeklySummary> {
    Ok(SiteWeeklySummary {
        week_start: week_start.to_string(),
        ..Default::default()
    })
}

#[cfg(feature = "postgres")]
async fn weekly_summary_postgres(
    connection_string: &str,
    week_start: &str,
) -> paperproof_sdk_rs::Result<SiteWeeklySummary> {
    let (client, connection) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
        .await
        .map_err(postgres_err("connect site analytics postgres"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("paperproof site analytics postgres connection closed: {error}");
        }
    });
    client
        .batch_execute(crate::schema::POSTGRES_REFERENCE_SCHEMA)
        .await
        .map_err(postgres_err("ensure site analytics postgres schema"))?;
    let visits = postgres_scalar(
        &client,
        "select count(*) from site_visit_events where week_start = $1",
        week_start,
    )
    .await?;
    let unique_visitors = postgres_scalar(
        &client,
        "select count(distinct coalesce(visitor_id_hash, fallback_visitor_key)) from site_visit_events where week_start = $1",
        week_start,
    )
    .await?;
    let unique_ips = postgres_scalar(
        &client,
        "select count(distinct ip_hash) from site_visit_events where week_start = $1 and ip_hash is not null",
        week_start,
    )
    .await?;
    let rows = client
        .query(
            "select path, count(*) from site_visit_events where week_start = $1 group by path order by count(*) desc, path asc limit 20",
            &[&week_start],
        )
        .await
        .map_err(postgres_err("query site analytics paths"))?;
    let top_paths = rows
        .iter()
        .map(|row| {
            Ok(SitePathSummary {
                path: row.get(0),
                visits: i64_to_u64(row.get(1))?,
            })
        })
        .collect::<paperproof_sdk_rs::Result<Vec<_>>>()?;
    let rows = client
        .query(
            "select coalesce(country, 'Unknown'), count(*) from site_visit_events where week_start = $1 and country is not null group by country order by count(*) desc, country asc limit 50",
            &[&week_start],
        )
        .await
        .map_err(postgres_err("query site analytics countries"))?;
    let countries = rows
        .iter()
        .map(|row| {
            Ok(SiteCountrySummary {
                country: row.get(0),
                visits: i64_to_u64(row.get(1))?,
            })
        })
        .collect::<paperproof_sdk_rs::Result<Vec<_>>>()?;
    Ok(SiteWeeklySummary {
        week_start: week_start.to_string(),
        visits,
        unique_visitors,
        unique_ips,
        top_paths,
        countries,
    })
}

#[cfg(not(feature = "postgres"))]
async fn weekly_summary_postgres(
    _connection_string: &str,
    week_start: &str,
) -> paperproof_sdk_rs::Result<SiteWeeklySummary> {
    Ok(SiteWeeklySummary {
        week_start: week_start.to_string(),
        ..Default::default()
    })
}

#[cfg(feature = "sqlite")]
fn sqlite_scalar(
    conn: &rusqlite::Connection,
    sql: &str,
    week_start: &str,
) -> paperproof_sdk_rs::Result<u64> {
    let value = conn
        .query_row(sql, [week_start], |row| row.get::<_, i64>(0))
        .map_err(sqlite_err("read site analytics scalar"))?;
    i64_to_u64(value)
}

#[cfg(feature = "sqlite")]
fn sqlite_top_paths(
    conn: &rusqlite::Connection,
    week_start: &str,
) -> paperproof_sdk_rs::Result<Vec<SitePathSummary>> {
    let mut stmt = conn
        .prepare(
            "select path, count(*) from site_visit_events where week_start = ?1 group by path order by count(*) desc, path asc limit 20",
        )
        .map_err(sqlite_err("prepare site analytics paths"))?;
    stmt.query_map([week_start], |row| {
        Ok(SitePathSummary {
            path: row.get(0)?,
            visits: i64_to_u64_sqlite(row.get(1)?)?,
        })
    })
    .map_err(sqlite_err("query site analytics paths"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sqlite_err("read site analytics paths"))
}

#[cfg(feature = "sqlite")]
fn sqlite_countries(
    conn: &rusqlite::Connection,
    week_start: &str,
) -> paperproof_sdk_rs::Result<Vec<SiteCountrySummary>> {
    let mut stmt = conn
        .prepare(
            "select coalesce(country, 'Unknown'), count(*) from site_visit_events where week_start = ?1 and country is not null group by country order by count(*) desc, country asc limit 50",
        )
        .map_err(sqlite_err("prepare site analytics countries"))?;
    stmt.query_map([week_start], |row| {
        Ok(SiteCountrySummary {
            country: row.get(0)?,
            visits: i64_to_u64_sqlite(row.get(1)?)?,
        })
    })
    .map_err(sqlite_err("query site analytics countries"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sqlite_err("read site analytics countries"))
}

fn hash_fields(salt: &str, fields: &[(&str, &str)]) -> String {
    let mut hasher = Sha256::new();
    hasher.update("paperproof-site-analytics-v1\n");
    hasher.update(salt.as_bytes());
    for (key, value) in fields {
        hasher.update(b"\n");
        hasher.update(key.as_bytes());
        hasher.update(b"=");
        hasher.update(value.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(value.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn clamp_text(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

fn canonical_ip(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('"')
        .to_string()
}

fn current_week_start_utc() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (now / 86_400) as i64;
    let monday_day = days - ((days + 3).rem_euclid(7));
    civil_from_days(monday_day)
}

fn current_timestamp_utc_plus_8() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    timestamp_with_offset(now, 8 * 3_600)
}

fn timestamp_with_offset(unix_seconds: u64, offset_seconds: i64) -> String {
    let adjusted = unix_seconds as i64 + offset_seconds;
    let days = adjusted.div_euclid(86_400);
    let seconds_of_day = adjusted.rem_euclid(86_400);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let date = civil_from_days(days);
    format!("{date} {hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days_since_unix_epoch: i64) -> String {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    format!("{year:04}-{m:02}-{d:02}")
}

fn i64_to_u64(value: i64) -> paperproof_sdk_rs::Result<u64> {
    u64::try_from(value).map_err(|_| {
        paperproof_sdk_rs::PaperProofError::invalid_input("site analytics count", "negative value")
    })
}

#[cfg(feature = "sqlite")]
fn i64_to_u64_sqlite(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Integer, Box::new(err))
    })
}

#[cfg(feature = "postgres")]
async fn postgres_scalar(
    client: &tokio_postgres::Client,
    sql: &str,
    week_start: &str,
) -> paperproof_sdk_rs::Result<u64> {
    let value = client
        .query_one(sql, &[&week_start])
        .await
        .map_err(postgres_err("read site analytics postgres scalar"))
        .map(|row| row.get::<_, i64>(0))?;
    i64_to_u64(value)
}

#[cfg(feature = "sqlite")]
fn sqlite_err(
    context: &'static str,
) -> impl Fn(rusqlite::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| paperproof_sdk_rs::PaperProofError::network(context, err.to_string())
}

#[cfg(feature = "postgres")]
fn postgres_err(
    context: &'static str,
) -> impl Fn(tokio_postgres::Error) -> paperproof_sdk_rs::PaperProofError {
    move |err| paperproof_sdk_rs::PaperProofError::network(context, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_stable_and_distinguish_structured_fields() {
        let a = hash_fields("salt", &[("kind", "visitor"), ("visitor_id", "abc")]);
        let b = hash_fields("salt", &[("kind", "visitor"), ("visitor_id", "abc")]);
        let c = hash_fields("salt", &[("kind", "visitor"), ("visitor_id", "abcd")]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn unix_week_start_uses_monday_utc() {
        assert_eq!(civil_from_days(0), "1970-01-01");
        let thursday_1970_01_01: i64 = 0;
        let monday = thursday_1970_01_01 - ((thursday_1970_01_01 + 3).rem_euclid(7));
        assert_eq!(civil_from_days(monday), "1969-12-29");
    }

    #[test]
    fn timestamp_offset_formats_as_utc_plus_8() {
        assert_eq!(timestamp_with_offset(0, 8 * 3_600), "1970-01-01 08:00:00");
        assert_eq!(
            timestamp_with_offset(86_399, 8 * 3_600),
            "1970-01-02 07:59:59"
        );
    }

    #[test]
    fn shared_ip_different_visitors_are_distinguishable() {
        let salt = "salt";
        let observed = ObservedVisit {
            client_ip: Some("203.0.113.10".to_string()),
            user_agent: Some("Browser".to_string()),
            accept_language: Some("en-US".to_string()),
        };
        let first = normalize_visit(
            salt,
            SiteVisitRequest {
                visitor_id: Some("visitor-a".to_string()),
                ..empty_request()
            },
            observed.clone(),
        );
        let second = normalize_visit(
            salt,
            SiteVisitRequest {
                visitor_id: Some("visitor-b".to_string()),
                ..empty_request()
            },
            observed,
        );
        assert_eq!(first.ip_hash, second.ip_hash);
        assert_ne!(first.visitor_id_hash, second.visitor_id_hash);
    }

    fn empty_request() -> SiteVisitRequest {
        SiteVisitRequest {
            visitor_id: None,
            path: Some("/".to_string()),
            referrer: None,
            timezone: None,
            language: None,
            screen: None,
            device_pixel_ratio: None,
            platform: None,
        }
    }
}
