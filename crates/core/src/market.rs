//! Marketplace plumbing (`spec.md` §7).
//!
//! Phase 4 ships signed index handling, a local cache, search, and install
//! delegation. There is no hosted registry and no background auto-update.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::{now_ms, EventBus};
use fergents_packages::{digest_of_bytes, is_newer, ArchiveLimits};
use fergents_plugins::PluginService;
use fergents_skills::SkillService;
use fergents_storage::{Storage, StorageError, Value};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

/// How long a cached index entry is considered fresh.
pub const CACHE_TTL_MS: i64 = 15 * 60 * 1000;

/// The supported marketplace index schema.
pub const INDEX_SCHEMA: u32 = 1;

/// A configured marketplace source.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceRecord {
    /// Stable id.
    pub id: String,
    /// URL or local path.
    pub url: String,
    /// `http`, `https`, `file`, or `git`.
    pub kind: String,
    /// Trusted signing key (a file under `trust/keys`).
    pub key_id: Option<String>,
    /// Whether refresh includes this source.
    pub enabled: bool,
    /// Last successful refresh.
    pub last_refreshed_at: Option<i64>,
    /// Last ETag.
    pub etag: Option<String>,
}

/// The download descriptor of one entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Download {
    /// Payload URL or path.
    pub url: String,
    /// Expected SHA-256 (lowercase hex).
    pub sha256: String,
    /// Expected size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
}

/// One marketplace entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketEntry {
    /// Package id.
    pub id: String,
    /// `plugin` or `skill`.
    #[serde(default = "default_entry_kind")]
    pub kind: String,
    /// Display name.
    pub name: String,
    /// Semver.
    pub version: String,
    /// Kernel API version.
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    /// One-line description.
    #[serde(default)]
    pub description: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Author.
    #[serde(default)]
    pub author: Option<String>,
    /// Homepage.
    #[serde(default)]
    pub homepage: Option<String>,
    /// Minimum kernel version.
    #[serde(default)]
    pub min_kernel: Option<String>,
    /// Download descriptor.
    pub download: Download,
    /// Detached payload signature.
    #[serde(default)]
    pub signature: Option<String>,
    /// Publication timestamp.
    #[serde(default)]
    pub published_at: Option<String>,
    /// Yanked entries cannot be installed.
    #[serde(default)]
    pub yanked: bool,
}

fn default_api_version() -> u32 {
    1
}

fn default_entry_kind() -> String {
    "plugin".to_string()
}

/// The signature envelope of an index document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexSignature {
    /// The signing key id (resolved under `trust/keys`).
    pub key_id: String,
    /// Ed25519 signature over the SHA-256 of the canonical entries JSON.
    pub signature: String,
}

/// A signed index document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexDocument {
    /// Index schema version.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The entries.
    pub entries: Vec<MarketEntry>,
    /// The detached signature, when the publisher signs the index.
    #[serde(default)]
    pub signature: Option<IndexSignature>,
}

fn default_schema() -> u32 {
    INDEX_SCHEMA
}

/// A cached entry as returned by search.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CachedEntry {
    /// The entry payload.
    #[serde(flatten)]
    pub entry: MarketEntry,
    /// The owning source.
    pub source_id: String,
    /// `plugin` or `skill`.
    pub kind: String,
    /// When it was fetched.
    pub fetched_at: i64,
    /// Whether the cache entry is past its TTL.
    pub stale: bool,
}

/// The outcome of refreshing one source.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefreshReport {
    /// The source id.
    pub source_id: String,
    /// Number of indexed entries.
    pub entries: usize,
    /// `verified`, `unsigned`, or `failed`.
    pub signature: String,
    /// The failure detail, when refresh failed.
    pub error: Option<String>,
}

/// A newer version available for an installed package.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateInfo {
    /// `plugin` or `skill`.
    pub kind: String,
    /// Package id.
    pub id: String,
    /// Installed version.
    pub installed: String,
    /// Newest published version.
    pub available: String,
}

/// Marketplace failures with stable codes.
#[derive(Debug, thiserror::Error)]
pub enum MarketError {
    /// Storage failure.
    #[error("marketplace storage error: {0}")]
    Storage(#[from] StorageError),
    /// Filesystem failure.
    #[error("marketplace io error: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid request.
    #[error("invalid marketplace request: {0}")]
    Invalid(String),
    /// Unknown source.
    #[error("unknown marketplace source: {0}")]
    UnknownSource(String),
    /// Unknown entry.
    #[error("unknown marketplace entry: {0}")]
    UnknownEntry(String),
    /// Fetch failure.
    #[error("marketplace fetch failed: {0}")]
    Fetch(String),
    /// The source requires a signature but the index is unsigned.
    #[error("marketplace index from {0} is unsigned")]
    Unsigned(String),
    /// The key is not in the trust store.
    #[error("unknown marketplace key: {0}")]
    UnknownKey(String),
    /// The index signature does not match.
    #[error("marketplace signature is invalid for source {0}")]
    InvalidSignature(String),
    /// The entry was yanked.
    #[error("marketplace entry {0} is yanked")]
    Yanked(String),
    /// The payload does not match its digest.
    #[error("marketplace payload hash mismatch: {0}")]
    HashMismatch(String),
    /// The source kind is not supported.
    #[error("unsupported marketplace source kind: {0}")]
    Unsupported(String),
    /// A package failure.
    #[error(transparent)]
    Package(#[from] fergents_packages::PackageError),
    /// A plugin failure.
    #[error(transparent)]
    Plugin(#[from] fergents_plugins::PluginError),
    /// A skill failure.
    #[error(transparent)]
    Skill(#[from] fergents_skills::SkillError),
}

impl MarketError {
    /// The stable error code returned over the protocol.
    pub fn code(&self) -> &'static str {
        match self {
            MarketError::Storage(_) => "marketplace_storage",
            MarketError::Io(_) => "marketplace_io",
            MarketError::Invalid(_) => "marketplace_invalid",
            MarketError::UnknownSource(_) => "marketplace_source_not_found",
            MarketError::UnknownEntry(_) => "marketplace_entry_not_found",
            MarketError::Fetch(_) => "marketplace_fetch_failed",
            MarketError::Unsigned(_) => "marketplace_unsigned",
            MarketError::UnknownKey(_) => "marketplace_unknown_key",
            MarketError::InvalidSignature(_) => "marketplace_signature_invalid",
            MarketError::Yanked(_) => "marketplace_yanked",
            MarketError::HashMismatch(_) => "marketplace_hash_mismatch",
            MarketError::Unsupported(_) => "marketplace_source_unsupported",
            MarketError::Package(_) => "package_invalid",
            MarketError::Plugin(error) => error.code(),
            MarketError::Skill(_) => "skill_invalid",
        }
    }
}

/// The marketplace service.
pub struct MarketService {
    data_dir: PathBuf,
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    plugins: Option<Arc<PluginService>>,
    skills: Option<Arc<SkillService>>,
    client: reqwest::Client,
}

impl MarketService {
    /// Build the service.
    pub fn new(
        data_dir: PathBuf,
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        plugins: Option<Arc<PluginService>>,
        skills: Option<Arc<SkillService>>,
    ) -> Self {
        Self {
            data_dir,
            storage,
            bus,
            plugins,
            skills,
            client: reqwest::Client::new(),
        }
    }

    /// Every configured source.
    pub fn sources(&self) -> Result<Vec<SourceRecord>, MarketError> {
        let rows = self
            .storage
            .query("SELECT * FROM marketplace_sources ORDER BY id", &[])?;
        Ok(rows.iter().filter_map(source_from_row).collect())
    }

    /// Add or replace a source.
    pub fn add_source(
        &self,
        id: &str,
        url: &str,
        kind: &str,
        key_id: Option<&str>,
    ) -> Result<SourceRecord, MarketError> {
        validate_source(id, url, kind)?;
        self.storage.execute(
            "INSERT INTO marketplace_sources (id, url, kind, key_id, enabled) \
             VALUES (?1, ?2, ?3, ?4, 1) \
             ON CONFLICT(id) DO UPDATE SET url = ?2, kind = ?3, key_id = ?4, enabled = 1",
            &[
                Value::String(id.to_string()),
                Value::String(url.to_string()),
                Value::String(kind.to_string()),
                key_id
                    .map(|key| Value::String(key.to_string()))
                    .unwrap_or(Value::Null),
            ],
        )?;
        self.source(id)
    }

    /// Remove a source and its cache.
    pub fn remove_source(&self, id: &str) -> Result<(), MarketError> {
        self.source(id)?;
        self.storage.execute(
            "DELETE FROM marketplace_cache WHERE source_id = ?1",
            &[Value::String(id.to_string())],
        )?;
        self.storage.execute(
            "DELETE FROM marketplace_sources WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(())
    }

    /// One source.
    pub fn source(&self, id: &str) -> Result<SourceRecord, MarketError> {
        let rows = self.storage.query(
            "SELECT * FROM marketplace_sources WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        rows.first()
            .and_then(source_from_row)
            .ok_or_else(|| MarketError::UnknownSource(id.to_string()))
    }

    /// Fetch and cache every enabled index (or just `source_id`).
    pub async fn refresh(
        &self,
        source_id: Option<&str>,
    ) -> Result<Vec<RefreshReport>, MarketError> {
        let sources = match source_id {
            Some(id) => vec![self.source(id)?],
            None => self.sources()?,
        };
        let mut reports = Vec::new();
        for source in sources.into_iter().filter(|source| source.enabled) {
            match self.refresh_one(&source).await {
                Ok(report) => reports.push(report),
                Err(error) => reports.push(RefreshReport {
                    source_id: source.id.clone(),
                    entries: 0,
                    signature: "failed".to_string(),
                    error: Some(format!("{}: {error}", error.code())),
                }),
            }
        }
        Ok(reports)
    }

    /// Search the cached index.
    pub fn search(
        &self,
        query: Option<&str>,
        kind: Option<&str>,
    ) -> Result<Vec<CachedEntry>, MarketError> {
        let rows = self.storage.query(
            "SELECT * FROM marketplace_cache ORDER BY kind, entry_id",
            &[],
        )?;
        let now = now_ms();
        let needle = query.map(|value| value.to_ascii_lowercase());
        let mut entries: Vec<CachedEntry> = rows
            .iter()
            .filter_map(cached_from_row)
            .filter(|cached| kind.is_none_or(|kind| cached.kind == kind))
            .filter(|cached| {
                let Some(query) = &needle else { return true };
                let entry = &cached.entry;
                entry.id.to_lowercase().contains(query)
                    || entry.name.to_lowercase().contains(query)
                    || entry
                        .description
                        .as_deref()
                        .is_some_and(|text| text.to_lowercase().contains(query))
                    || entry
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(query))
            })
            .map(|mut cached| {
                cached.stale = now - cached.fetched_at > CACHE_TTL_MS;
                cached
            })
            .collect();
        entries.sort_by(|a, b| a.entry.name.cmp(&b.entry.name));
        Ok(entries)
    }

    /// One cached entry.
    pub fn get(&self, source_id: &str, kind: &str, id: &str) -> Result<CachedEntry, MarketError> {
        let rows = self.storage.query(
            "SELECT * FROM marketplace_cache WHERE source_id = ?1 AND kind = ?2 AND entry_id = ?3",
            &[
                Value::String(source_id.to_string()),
                Value::String(kind.to_string()),
                Value::String(id.to_string()),
            ],
        )?;
        rows.first()
            .and_then(cached_from_row)
            .ok_or_else(|| MarketError::UnknownEntry(format!("{kind}/{id}")))
            .map(|mut cached| {
                cached.stale = now_ms() - cached.fetched_at > CACHE_TTL_MS;
                cached
            })
    }

    /// Download and unpack an entry, then produce an install review.
    ///
    /// Nothing is installed here: the caller must approve the review through
    /// `plugin.install` or `skill.install`.
    pub async fn install(
        &self,
        source_id: &str,
        kind: &str,
        id: &str,
    ) -> Result<JsonValue, MarketError> {
        let cached = self.get(source_id, kind, id)?;
        if cached.entry.yanked {
            return Err(MarketError::Yanked(format!("{kind}/{id}")));
        }
        let bytes = self.fetch(&cached.entry.download.url).await?;
        let digest = to_hex(&digest_of_bytes(&bytes));
        if !digest.eq_ignore_ascii_case(&cached.entry.download.sha256) {
            return Err(MarketError::HashMismatch(format!("{kind}/{id}")));
        }
        if let Some(size) = cached.entry.download.size {
            if size != bytes.len() as u64 {
                return Err(MarketError::HashMismatch(format!(
                    "{kind}/{id}: expected {size} bytes"
                )));
            }
        }
        let dir = self
            .data_dir
            .join("cache")
            .join("marketplace")
            .join(format!("{}-{}", cached.entry.id, cached.entry.version));
        let _ = fs::remove_dir_all(&dir);
        fergents_packages::extract_archive(&bytes, &dir, &ArchiveLimits::default())?;
        match kind {
            "plugin" => {
                let service = self
                    .plugins
                    .clone()
                    .ok_or_else(|| MarketError::Unsupported("capability.plugins".to_string()))?;
                let review = service.inspect(&fergents_plugins::PluginSource::Dir(dir.clone()))?;
                Ok(json!({
                    "kind": "plugin",
                    "requires_approval": true,
                    "entry": cached,
                    "review": review,
                }))
            }
            "skill" => {
                let service = self
                    .skills
                    .clone()
                    .ok_or_else(|| MarketError::Unsupported("capability.skills".to_string()))?;
                let review = service.inspect(&fergents_skills::SkillSource::Dir(dir.clone()))?;
                Ok(json!({
                    "kind": "skill",
                    "requires_approval": true,
                    "entry": cached,
                    "review": review,
                }))
            }
            other => Err(MarketError::Unsupported(other.to_string())),
        }
    }

    /// Compare installed versions with the cached index.
    pub fn check_updates(&self) -> Result<Vec<UpdateInfo>, MarketError> {
        let cached = self.search(None, None)?;
        let mut updates = Vec::new();
        if let Some(plugins) = &self.plugins {
            for plugin in plugins.list()? {
                if let Some(info) =
                    newest(&cached, "plugin", &plugin.record.id, &plugin.record.version)
                {
                    updates.push(info);
                }
            }
        }
        if let Some(skills) = &self.skills {
            for skill in skills.list()? {
                if let Some(info) =
                    newest(&cached, "skill", &skill.record.id, &skill.record.version)
                {
                    updates.push(info);
                }
            }
        }
        if !updates.is_empty() {
            let _ = self.bus.publish(
                "marketplace.update.available".to_string(),
                "marketplace".to_string(),
                json!({ "updates": updates }),
            );
        }
        Ok(updates)
    }

    async fn refresh_one(&self, source: &SourceRecord) -> Result<RefreshReport, MarketError> {
        let bytes = self.fetch(&source.url).await?;
        let document: IndexDocument = serde_json::from_slice(&bytes)
            .map_err(|error| MarketError::Invalid(format!("invalid index: {error}")))?;
        if document.schema != INDEX_SCHEMA {
            return Err(MarketError::Invalid(format!(
                "unsupported index schema {}",
                document.schema
            )));
        }
        let signature = self.verify_index(source, &document)?;
        self.storage.execute(
            "DELETE FROM marketplace_cache WHERE source_id = ?1",
            &[Value::String(source.id.clone())],
        )?;
        let fetched_at = now_ms();
        for entry in &document.entries {
            self.storage.execute(
                "INSERT INTO marketplace_cache (source_id, entry_id, kind, entry, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                &[
                    Value::String(source.id.clone()),
                    Value::String(entry.id.clone()),
                    Value::String(entry.kind.clone()),
                    Value::String(serde_json::to_string(entry).unwrap_or_default()),
                    Value::from(fetched_at),
                ],
            )?;
        }
        self.storage.execute(
            "UPDATE marketplace_sources SET last_refreshed_at = ?1 WHERE id = ?2",
            &[Value::from(fetched_at), Value::String(source.id.clone())],
        )?;
        let _ = self.bus.publish(
            "marketplace.index.refreshed".to_string(),
            "marketplace".to_string(),
            json!({ "source_id": source.id, "entries": document.entries.len() }),
        );
        Ok(RefreshReport {
            source_id: source.id.clone(),
            entries: document.entries.len(),
            signature: signature.to_string(),
            error: None,
        })
    }

    fn verify_index(
        &self,
        source: &SourceRecord,
        document: &IndexDocument,
    ) -> Result<&'static str, MarketError> {
        let Some(signature) = &document.signature else {
            return if source.key_id.is_some() {
                Err(MarketError::Unsigned(source.id.clone()))
            } else {
                Ok("unsigned")
            };
        };
        let key_id = source.key_id.as_deref().unwrap_or(&signature.key_id);
        let path = self
            .data_dir
            .join("trust")
            .join("keys")
            .join(format!("{key_id}.pub"));
        let Ok(public_key) = fs::read_to_string(&path) else {
            return Err(MarketError::UnknownKey(key_id.to_string()));
        };
        let bytes = serde_json::to_vec(&document.entries)
            .map_err(|error| MarketError::Invalid(error.to_string()))?;
        fergents_packages::verify_digest(
            &digest_of_bytes(&bytes),
            public_key.trim(),
            &signature.signature,
        )
        .map_err(|_| MarketError::InvalidSignature(source.id.clone()))?;
        Ok("verified")
    }

    async fn fetch(&self, url: &str) -> Result<Vec<u8>, MarketError> {
        let url = url.trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|error| MarketError::Fetch(error.to_string()))?;
            let status = response.status();
            let bytes = response
                .bytes()
                .await
                .map_err(|error| MarketError::Fetch(error.to_string()))?;
            if !status.is_success() {
                return Err(MarketError::Fetch(format!("http {status}: {url}")));
            }
            return Ok(bytes.to_vec());
        }
        if let Some(path) = url.strip_prefix("file://") {
            return fs::read(path).map_err(MarketError::Io);
        }
        let path = Path::new(url);
        if path.is_file() {
            return fs::read(path).map_err(MarketError::Io);
        }
        Err(MarketError::Unsupported(url.to_string()))
    }
}

fn source_from_row(row: &fergents_storage::Row) -> Option<SourceRecord> {
    Some(SourceRecord {
        id: row.get("id").and_then(Value::as_str)?.to_string(),
        url: row.get("url").and_then(Value::as_str)?.to_string(),
        kind: row.get("kind").and_then(Value::as_str)?.to_string(),
        key_id: row
            .get("key_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        enabled: row.get("enabled").and_then(Value::as_i64).unwrap_or(1) != 0,
        last_refreshed_at: row.get("last_refreshed_at").and_then(Value::as_i64),
        etag: row.get("etag").and_then(Value::as_str).map(str::to_string),
    })
}

fn cached_from_row(row: &fergents_storage::Row) -> Option<CachedEntry> {
    let entry = row
        .get("entry")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<MarketEntry>(text).ok())?;
    Some(CachedEntry {
        entry,
        source_id: row.get("source_id").and_then(Value::as_str)?.to_string(),
        kind: row.get("kind").and_then(Value::as_str)?.to_string(),
        fetched_at: row.get("fetched_at").and_then(Value::as_i64).unwrap_or(0),
        stale: false,
    })
}

fn newest(cached: &[CachedEntry], kind: &str, id: &str, installed: &str) -> Option<UpdateInfo> {
    let mut best: Option<&CachedEntry> = None;
    for entry in cached {
        if entry.kind != kind || entry.entry.id != id || entry.entry.yanked {
            continue;
        }
        if !is_newer(&entry.entry.version, installed) {
            continue;
        }
        if best.is_none_or(|current| is_newer(&entry.entry.version, &current.entry.version)) {
            best = Some(entry);
        }
    }
    best.map(|entry| UpdateInfo {
        kind: kind.to_string(),
        id: id.to_string(),
        installed: installed.to_string(),
        available: entry.entry.version.clone(),
    })
}

fn validate_source(id: &str, url: &str, kind: &str) -> Result<(), MarketError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.');
    if !valid {
        return Err(MarketError::Invalid(format!("invalid source id: {id:?}")));
    }
    if url.trim().is_empty() {
        return Err(MarketError::Invalid("missing source url".to_string()));
    }
    if !matches!(kind, "http" | "https" | "file" | "git") {
        return Err(MarketError::Unsupported(kind.to_string()));
    }
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use fergents_events::EventBus;
    use std::io::Write;

    fn service(dir: &Path) -> MarketService {
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("migrations");
        let storage = Arc::new(
            fergents_storage::SqliteStorage::open(&dir.join("market.db"), &migrations).unwrap(),
        );
        let bus = Arc::new(EventBus::new(storage.clone(), 8));
        MarketService::new(dir.to_path_buf(), storage, bus, None, None)
    }

    fn write_index(dir: &Path, body: &str) -> String {
        let file = dir.join("index.json");
        let mut handle = std::fs::File::create(&file).unwrap();
        handle.write_all(body.as_bytes()).unwrap();
        file.to_string_lossy().into_owned()
    }

    const ECHO: &str = r#"{
        "id": "com.example.echo",
        "kind": "plugin",
        "name": "Echo",
        "version": "1.2.0",
        "description": "Repeats text",
        "tags": ["text"],
        "download": { "url": "https://example.com/echo.fergplugin", "sha256": "00", "size": 1 }
    }"#;

    #[tokio::test]
    async fn refreshes_searches_and_reads_a_local_index() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        let url = write_index(
            dir.path(),
            &format!("{{\"schema\":1,\"entries\":[{ECHO}]}}"),
        );

        let source = service.add_source("local", &url, "file", None).unwrap();
        assert_eq!(source.id, "local");

        let reports = service.refresh(None).await.unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].entries, 1);
        assert_eq!(reports[0].signature, "unsigned");

        let found = service.search(Some("echo"), None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].entry.version, "1.2.0");
        assert!(!found[0].stale);
        assert!(service.search(Some("missing"), None).unwrap().is_empty());
        assert_eq!(
            service
                .get("local", "plugin", "com.example.echo")
                .unwrap()
                .entry
                .name,
            "Echo"
        );

        service.remove_source("local").unwrap();
        assert!(service.sources().unwrap().is_empty());
        assert!(service.search(None, None).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_keyed_source_rejects_an_unsigned_index() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        let url = write_index(dir.path(), "{\"schema\":1,\"entries\":[]}");
        service
            .add_source("signed", &url, "file", Some("acme"))
            .unwrap();

        let reports = service.refresh(Some("signed")).await.unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].signature, "failed");
        assert!(reports[0]
            .error
            .as_deref()
            .unwrap()
            .contains("marketplace_unsigned"));
    }

    #[tokio::test]
    async fn refuses_yanked_entries() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        service
            .add_source("local", "/tmp/index.json", "file", None)
            .unwrap();
        let entry = serde_json::json!({
            "id": "com.example.gone",
            "kind": "plugin",
            "name": "Gone",
            "version": "1.0.0",
            "download": { "url": "https://example.com/gone.fergplugin", "sha256": "00" },
            "yanked": true,
        });
        service
            .storage
            .execute(
                "INSERT INTO marketplace_cache (source_id, entry_id, kind, entry, fetched_at) \
                 VALUES ('local', 'com.example.gone', 'plugin', ?1, ?2)",
                &[Value::String(entry.to_string()), Value::from(now_ms())],
            )
            .unwrap();

        let error = service.install("local", "plugin", "com.example.gone").await;
        assert!(matches!(error, Err(MarketError::Yanked(_))));
    }
}
