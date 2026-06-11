use figment::{
    providers::{Env, Format, Toml, Yaml},
    Figment,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    /// AES-256-GCM key — 64 hex chars (32 bytes). Required.
    pub credential_encryption_key: String,
    /// JWT signing secret. Required.
    pub jwt_secret: String,
    /// Data directory for SQLite DBs and blob storage.
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Bind host.
    #[serde(default = "default_host")]
    pub server_host: String,
    /// Bind port.
    #[serde(default = "default_port")]
    pub server_port: u16,
    /// Base URL used in OAuth redirect URIs.
    #[serde(default = "default_base_url")]
    pub app_base_url: String,
    pub google_oauth_client_id: Option<String>,
    pub google_oauth_client_secret: Option<String>,
    pub microsoft_oauth_client_id: Option<String>,
    pub microsoft_oauth_client_secret: Option<String>,
    /// Periodically download the OpenPhish community feed for link-reputation
    /// checks (outbound GET only; no user data leaves the server).
    #[serde(default = "default_true")]
    pub openphish_enabled: bool,
    #[serde(default = "default_openphish_feed_url")]
    pub openphish_feed_url: String,
}

fn default_data_dir() -> String { "./data".into() }
fn default_host() -> String { "0.0.0.0".into() }
fn default_port() -> u16 { 8080 }
fn default_base_url() -> String { "http://localhost:8080".into() }
fn default_true() -> bool { true }
fn default_openphish_feed_url() -> String { "https://openphish.com/feed.txt".into() }

impl Settings {
    /// Load configuration (later sources win):
    ///   mailquill.toml / mailquill.yaml in parent dir (project root)
    ///   → mailquill.toml / mailquill.yaml in cwd (backend/)
    ///   → environment variables  (CREDENTIAL_ENCRYPTION_KEY, JWT_SECRET, …)
    ///
    /// All keys are case-insensitive; use `__` in env var names as the
    /// hierarchy separator for future nested keys (e.g. `GOOGLE__CLIENT_ID`).
    /// Missing files are silently skipped.
    pub fn load() -> Self {
        Figment::new()
            .merge(Toml::file("../mailquill.toml"))
            .merge(Yaml::file("../mailquill.yaml"))
            .merge(Toml::file("mailquill.toml"))
            .merge(Yaml::file("mailquill.yaml"))
            .merge(Env::raw().split("__"))
            .extract()
            .unwrap_or_else(|e| panic!("configuration error: {e}"))
    }
}
