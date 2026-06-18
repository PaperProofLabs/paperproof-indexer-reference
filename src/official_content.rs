// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::normalized::VersionRecord;
use paperproof_sdk_rs::PaperProofQueryClient;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialContentResponse {
    pub surface: String,
    pub slug: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub markdown: String,
    pub artifact_code: Option<String>,
    pub series_id: String,
    pub version_id: String,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub blob_id: String,
    pub content_hash: String,
    pub content_type: Option<String>,
    pub verification_status: String,
    pub render_status: String,
    pub source_kind: String,
    pub has_local_asset_refs: bool,
    pub manifest_entry: Value,
}

pub type OfficialContentCache = Arc<RwLock<HashMap<String, OfficialContentResponse>>>;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OfficialContentWarmupReport {
    pub attempted: usize,
    pub cached: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialDocsManifest {
    pub sections: Vec<OfficialDocsSection>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialDocsSection {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub artifact_code: Option<String>,
    pub series_id: Option<String>,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_content_hash: Option<String>,
    pub content_type: Option<String>,
    #[serde(default)]
    pub topics: Vec<OfficialDocsTopic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialDocsTopic {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub artifact_code: Option<String>,
    pub series_id: Option<String>,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_content_hash: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialBlogManifest {
    pub posts: Vec<OfficialBlogPost>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialBlogPost {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub artifact_code: Option<String>,
    pub series_id: Option<String>,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_content_hash: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialForumManifest {
    pub sections: Vec<OfficialForumSection>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialForumSection {
    pub topics: Vec<OfficialForumTopic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialForumTopic {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub artifact_code: Option<String>,
    pub series_id: Option<String>,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_content_hash: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OfficialContentConfig {
    pub manifest_base_url: String,
    pub walrus_aggregator_url: String,
}

impl Default for OfficialContentConfig {
    fn default() -> Self {
        Self {
            manifest_base_url: "https://paperproof.site".to_string(),
            walrus_aggregator_url: "https://aggregator.walrus-mainnet.walrus.space".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct OfficialContentService {
    http: reqwest::Client,
    config: OfficialContentConfig,
}

impl OfficialContentService {
    pub fn new(config: OfficialContentConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    pub async fn load_manifest<T>(&self, path: &str) -> paperproof_sdk_rs::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!(
            "{}/{}",
            self.config.manifest_base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let response =
            self.http.get(&url).send().await.map_err(|err| {
                paperproof_sdk_rs::PaperProofError::network(&url, err.to_string())
            })?;
        if !response.status().is_success() {
            return Err(paperproof_sdk_rs::PaperProofError::network(
                &url,
                format!("HTTP {}", response.status()),
            ));
        }
        response.json::<T>().await.map_err(|err| {
            paperproof_sdk_rs::PaperProofError::invalid_input("official manifest", err.to_string())
        })
    }

    pub async fn render_entry(
        &self,
        surface: &str,
        slug: &str,
        entry: OfficialEntry,
        versions: Vec<VersionRecord>,
    ) -> paperproof_sdk_rs::Result<OfficialContentResponse> {
        let series_id = entry.series_id.clone().ok_or_else(|| {
            paperproof_sdk_rs::PaperProofError::invalid_input(
                "seriesId",
                "official entry has no seriesId",
            )
        })?;
        let (version, source_kind) = match choose_version(&versions) {
            Some(version) if version.walrus_blob_id.is_some() => {
                (version, "normalized".to_string())
            }
            None => (
                read_version_fallback(&series_id, entry.current_version_id.as_deref()).await?,
                "sdk_fallback".to_string(),
            ),
            Some(_) => (
                read_version_fallback(&series_id, entry.current_version_id.as_deref()).await?,
                "sdk_fallback".to_string(),
            ),
        };
        let blob_id = version.walrus_blob_id.clone().ok_or_else(|| {
            paperproof_sdk_rs::PaperProofError::invalid_input(
                "walrusBlobId",
                "indexed version has no Walrus blob ID",
            )
        })?;
        let bytes = self.read_walrus_blob(&blob_id).await?;
        let actual_hash = format!("sha256:{}", sha256_hex(&bytes));
        let expected_hash = version
            .content_hash
            .clone()
            .or(entry.latest_content_hash.clone());
        if let Some(expected) = expected_hash {
            if !hash_eq(&expected, &actual_hash) {
                return Err(paperproof_sdk_rs::PaperProofError::invalid_input(
                    "content hash",
                    format!("expected {expected}, got {actual_hash}"),
                ));
            }
        }
        let raw_markdown = markdown_from_package_or_text(&bytes)?;
        let markdown = if surface == "blog" || surface == "forum" {
            clean_blog_markdown_for_display(
                &raw_markdown,
                entry.title.as_deref().unwrap_or_default(),
            )
        } else {
            raw_markdown
        };
        let has_local_asset_refs = markdown_has_local_asset_refs(&markdown);
        Ok(OfficialContentResponse {
            surface: surface.to_string(),
            slug: slug.to_string(),
            title: entry.title,
            summary: entry.summary,
            markdown,
            artifact_code: entry.artifact_code,
            series_id,
            version_id: version.version_id,
            comments_tree_id: entry.comments_tree_id,
            likes_book_id: entry.likes_book_id,
            blob_id,
            content_hash: actual_hash,
            content_type: version.content_type.or(entry.content_type),
            verification_status: "verified".to_string(),
            render_status: "markdown".to_string(),
            source_kind,
            has_local_asset_refs,
            manifest_entry: entry.manifest_entry,
        })
    }

    async fn read_walrus_blob(&self, blob_id: &str) -> paperproof_sdk_rs::Result<Vec<u8>> {
        let url = format!(
            "{}/v1/blobs/{}",
            self.config.walrus_aggregator_url.trim_end_matches('/'),
            blob_id
        );
        let response =
            self.http.get(&url).send().await.map_err(|err| {
                paperproof_sdk_rs::PaperProofError::network(&url, err.to_string())
            })?;
        if !response.status().is_success() {
            return Err(paperproof_sdk_rs::PaperProofError::network(
                &url,
                format!("HTTP {}", response.status()),
            ));
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|err| paperproof_sdk_rs::PaperProofError::network(&url, err.to_string()))
    }
}

#[derive(Clone, Debug)]
pub struct OfficialEntry {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub artifact_code: Option<String>,
    pub series_id: Option<String>,
    pub comments_tree_id: Option<String>,
    pub likes_book_id: Option<String>,
    pub current_version_id: Option<String>,
    pub latest_content_hash: Option<String>,
    pub content_type: Option<String>,
    pub manifest_entry: Value,
}

impl From<OfficialDocsSection> for OfficialEntry {
    fn from(value: OfficialDocsSection) -> Self {
        let manifest_entry = serde_json::to_value(&value).unwrap_or(Value::Null);
        Self {
            title: Some(value.title),
            summary: value.summary,
            artifact_code: value.artifact_code,
            series_id: value.series_id,
            comments_tree_id: value.comments_tree_id,
            likes_book_id: value.likes_book_id,
            current_version_id: value.current_version_id,
            latest_content_hash: value.latest_content_hash,
            content_type: value.content_type,
            manifest_entry,
        }
    }
}

impl From<OfficialDocsTopic> for OfficialEntry {
    fn from(value: OfficialDocsTopic) -> Self {
        let manifest_entry = serde_json::to_value(&value).unwrap_or(Value::Null);
        Self {
            title: Some(value.title),
            summary: value.summary,
            artifact_code: value.artifact_code,
            series_id: value.series_id,
            comments_tree_id: value.comments_tree_id,
            likes_book_id: value.likes_book_id,
            current_version_id: value.current_version_id,
            latest_content_hash: value.latest_content_hash,
            content_type: value.content_type,
            manifest_entry,
        }
    }
}

impl From<OfficialBlogPost> for OfficialEntry {
    fn from(value: OfficialBlogPost) -> Self {
        let manifest_entry = serde_json::to_value(&value).unwrap_or(Value::Null);
        Self {
            title: Some(value.title),
            summary: value.summary,
            artifact_code: value.artifact_code,
            series_id: value.series_id,
            comments_tree_id: value.comments_tree_id,
            likes_book_id: value.likes_book_id,
            current_version_id: value.current_version_id,
            latest_content_hash: value.latest_content_hash,
            content_type: value.content_type,
            manifest_entry,
        }
    }
}

impl From<OfficialForumTopic> for OfficialEntry {
    fn from(value: OfficialForumTopic) -> Self {
        let manifest_entry = serde_json::to_value(&value).unwrap_or(Value::Null);
        Self {
            title: Some(value.title),
            summary: value.summary,
            artifact_code: value.artifact_code,
            series_id: value.series_id,
            comments_tree_id: value.comments_tree_id,
            likes_book_id: value.likes_book_id,
            current_version_id: value.current_version_id,
            latest_content_hash: value.latest_content_hash,
            content_type: value.content_type,
            manifest_entry,
        }
    }
}

pub fn docs_entry(
    manifest: OfficialDocsManifest,
    section: &str,
    topic: Option<&str>,
) -> Option<OfficialEntry> {
    let section = manifest
        .sections
        .into_iter()
        .find(|item| item.id == section)?;
    if let Some(topic_id) = topic {
        section
            .topics
            .into_iter()
            .find(|item| item.id == topic_id)
            .map(OfficialEntry::from)
    } else {
        Some(OfficialEntry::from(section))
    }
}

pub fn docs_entries(manifest: OfficialDocsManifest) -> Vec<(String, OfficialEntry)> {
    let mut entries = Vec::new();
    for section in manifest.sections {
        let section_slug = section.id.clone();
        let topics = section.topics.clone();
        entries.push((section_slug.clone(), OfficialEntry::from(section)));
        entries.extend(topics.into_iter().map(|topic| {
            let slug = format!("{section_slug}/{}", topic.id);
            (slug, OfficialEntry::from(topic))
        }));
    }
    entries
}

pub fn blog_entry(manifest: OfficialBlogManifest, slug: &str) -> Option<OfficialEntry> {
    manifest
        .posts
        .into_iter()
        .find(|item| item.id == slug)
        .map(OfficialEntry::from)
}

pub fn blog_entries(manifest: OfficialBlogManifest) -> Vec<(String, OfficialEntry)> {
    manifest
        .posts
        .into_iter()
        .map(|post| (post.id.clone(), OfficialEntry::from(post)))
        .collect()
}

pub fn forum_entry(manifest: OfficialForumManifest, slug: &str) -> Option<OfficialEntry> {
    manifest
        .sections
        .into_iter()
        .flat_map(|section| section.topics)
        .find(|item| item.id == slug)
        .map(OfficialEntry::from)
}

pub fn forum_entries(manifest: OfficialForumManifest) -> Vec<(String, OfficialEntry)> {
    manifest
        .sections
        .into_iter()
        .flat_map(|section| section.topics)
        .map(|topic| (topic.id.clone(), OfficialEntry::from(topic)))
        .collect()
}

pub fn official_cache_key(surface: &str, slug: &str) -> String {
    format!("{}:{}", surface, slug.trim_matches('/'))
}

fn choose_version(versions: &[VersionRecord]) -> Option<VersionRecord> {
    versions
        .iter()
        .max_by_key(|item| item.version.unwrap_or(0))
        .cloned()
}

async fn read_version_fallback(
    series_id: &str,
    manifest_version_id: Option<&str>,
) -> paperproof_sdk_rs::Result<VersionRecord> {
    let query = PaperProofQueryClient::mainnet();
    let series = query.read.get_series_view(series_id).await?;
    let version_id = series
        .current_version_id
        .as_deref()
        .or(manifest_version_id)
        .ok_or_else(|| {
            paperproof_sdk_rs::PaperProofError::invalid_input(
                "version",
                "official entry has no indexed, series current, or manifest currentVersionId",
            )
        })?;
    let version = query.read.get_version_view(version_id).await?;
    let walrus_blob_id = string_path(&version.raw_fields, &["header", "fields", "walrus_blob_id"])
        .or_else(|| string_path(&version.raw_fields, &["header", "walrus_blob_id"]));
    Ok(VersionRecord {
        version_id: version.id,
        series_id: version.series_id.unwrap_or_default(),
        artifact_type: version.artifact_type.map(u64::from),
        version: version.version,
        content_hash: version.content_hash,
        walrus_blob_id,
        content_type: None,
        created_at: None,
        raw_json: version.raw_fields,
    })
}

fn string_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor.as_str().map(ToString::to_string).or_else(|| {
        cursor
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_eq(expected: &str, actual: &str) -> bool {
    normalize_hash(expected).eq_ignore_ascii_case(&normalize_hash(actual))
}

fn normalize_hash(value: &str) -> String {
    value.strip_prefix("sha256:").unwrap_or(value).to_string()
}

fn markdown_from_package_or_text(bytes: &[u8]) -> paperproof_sdk_rs::Result<String> {
    if !is_zip_bytes(bytes) {
        return String::from_utf8(bytes.to_vec()).map_err(|err| {
            paperproof_sdk_rs::PaperProofError::invalid_input(
                "official content utf8",
                err.to_string(),
            )
        });
    }
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|err| {
        paperproof_sdk_rs::PaperProofError::invalid_input("markdown package", err.to_string())
    })?;
    let entry = package_entry_name(&mut archive).unwrap_or_else(|| "index.md".to_string());
    let selected_entry = if archive.by_name(&entry).is_ok() {
        entry
    } else {
        "index.md".to_string()
    };
    let mut file = archive.by_name(&selected_entry).map_err(|err| {
        paperproof_sdk_rs::PaperProofError::invalid_input("markdown package", err.to_string())
    })?;
    let mut markdown = String::new();
    file.read_to_string(&mut markdown).map_err(|err| {
        paperproof_sdk_rs::PaperProofError::invalid_input("markdown package utf8", err.to_string())
    })?;
    Ok(markdown)
}

fn package_entry_name<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<String> {
    let mut manifest_file = archive.by_name("manifest.json").ok()?;
    let mut manifest = String::new();
    manifest_file.read_to_string(&mut manifest).ok()?;
    let value: Value = serde_json::from_str(&manifest).ok()?;
    value
        .get("entry")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn is_zip_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4b && bytes[2] == 0x03 && bytes[3] == 0x04
}

fn clean_blog_markdown_for_display(markdown: &str, title: &str) -> String {
    let mut lines: Vec<&str> = markdown.lines().collect();
    if lines
        .first()
        .map(|line| line.trim() == format!("# {title}"))
        .unwrap_or(false)
    {
        lines.remove(0);
        if lines
            .first()
            .map(|line| line.trim().is_empty())
            .unwrap_or(false)
        {
            lines.remove(0);
        }
    }
    lines
        .into_iter()
        .filter(|line| {
            let trimmed = line.trim().to_ascii_lowercase();
            !trimmed.starts_with("status:") && !trimmed.starts_with("suggested artifact type:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_start()
        .to_string()
}

fn markdown_has_local_asset_refs(markdown: &str) -> bool {
    markdown.lines().any(|line| {
        let Some(start) = line.find("![") else {
            return false;
        };
        let rest = &line[start..];
        let Some(open) = rest.find("](") else {
            return false;
        };
        let after_open = &rest[(open + 2)..];
        let Some(close) = after_open.find(')') else {
            return false;
        };
        let target = after_open[..close].trim();
        !target.is_empty()
            && !target.starts_with("http://")
            && !target.starts_with("https://")
            && !target.starts_with("data:")
            && !target.starts_with('#')
            && !target.starts_with('/')
    })
}
