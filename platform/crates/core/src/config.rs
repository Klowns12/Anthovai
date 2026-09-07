//! Configuration loading: `config/default.toml`, then `config/{env}.toml`,
//! then environment variables prefixed `ANTHOVAI__` (double underscore separates
//! nesting, e.g. `ANTHOVAI__DATABASE__URL`).
//!
//! Secrets are never read from the TOML files. They are named there by env-var
//! name and resolved at load time.

use serde::Deserialize;

/// Read `.env` into the process environment, if there is one.
///
/// `.env.example` has said "copy to .env for local development" since the first
/// commit, and nothing read it — so a key placed there did nothing, silently,
/// and the only symptom was every question failing with `provider_unavailable`.
///
/// A real environment variable always wins over the file. That is what makes
/// this safe to call in production too: a container's injected secrets are not
/// quietly replaced by a `.env` that happened to be baked into the image.
///
/// Called from `main`, not from `Settings::load`, because putting values into
/// the process environment is a side effect and a library function that reads
/// configuration should not have one.
pub fn load_dotenv() {
    match dotenvy::dotenv() {
        Ok(path) => tracing::info!(path = %path.display(), "loaded .env"),
        // Absent is the normal case in a deployment, and not worth a line.
        Err(e) if e.not_found() => {}
        Err(e) => {
            tracing::warn!(error = %e, "could not read .env; continuing with the environment as it is")
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub storage: StorageSettings,
    pub embeddings: EmbeddingSettings,
    pub retrieval: RetrievalSettings,
    pub auth: AuthSettings,
    pub worker: WorkerSettings,
    #[serde(default)]
    pub providers: ProviderSettings,
    #[serde(default)]
    pub mail: MailSettings,
}

/// Where confirmation emails go out through.
///
/// Every field optional: a deployment without a mail provider still starts, and
/// logs the messages it would have sent rather than refusing to run. That is
/// the right trade for the one email this platform sends — but it is announced
/// at startup, because silently not sending is indistinguishable from sending.
#[derive(Clone, Debug, Deserialize)]
pub struct MailSettings {
    /// `smtp.provider.com:587`. Empty or absent means nothing is sent.
    #[serde(default)]
    pub smtp_url: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_mail_from")]
    pub from: String,
    /// Where a confirmation link points — the website, not this API. The link
    /// is followed by a person in a browser, and the page that thanks them
    /// belongs to the site they signed up on.
    #[serde(default = "default_site_url")]
    pub site_url: String,
}

fn default_mail_from() -> String {
    "Anthovai <no-reply@anthovai.com>".to_owned()
}

fn default_site_url() -> String {
    "http://localhost:3000".to_owned()
}

impl Default for MailSettings {
    fn default() -> Self {
        Self {
            smtp_url: None,
            username: None,
            password: None,
            from: default_mail_from(),
            site_url: default_site_url(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub dashboard_origins: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
    #[serde(default)]
    pub run_migrations_on_start: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StorageSettings {
    pub provider: String,
    pub bucket: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    /// Where `provider = "local"` keeps its files. A developer can then run the
    /// whole platform with only PostgreSQL on the machine.
    #[serde(default = "default_local_path")]
    pub local_path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmbeddingSettings {
    pub default_model: String,
    pub dimension: usize,
    pub batch_size: usize,
    pub concurrency: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RetrievalSettings {
    pub vector_top: usize,
    pub keyword_top: usize,
    pub rrf_k: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthSettings {
    pub session_ttl_hours: u64,
    pub api_key_cache_secs: u64,
    pub argon2_memory_kib: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkerSettings {
    pub concurrency: usize,
    pub poll_interval_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProviderSettings {
    #[serde(default)]
    pub openai: Option<ProviderEntry>,
    #[serde(default)]
    pub anthropic: Option<ProviderEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProviderEntry {
    /// Name of the environment variable holding the key, not the key itself.
    pub api_key_env: String,
    pub base_url: String,
    #[serde(default)]
    pub enabled: bool,
}

impl ProviderEntry {
    /// Resolve the secret from the environment. `None` means this provider is
    /// simply not configured on this deployment.
    pub fn api_key(&self) -> Option<String> {
        std::env::var(&self.api_key_env)
            .ok()
            .filter(|v| !v.is_empty())
    }
}

fn default_region() -> String {
    "auto".to_owned()
}

fn default_local_path() -> String {
    "./data/storage".to_owned()
}

impl Settings {
    /// Load from `config/` relative to the current working directory.
    pub fn load() -> Result<Self, config::ConfigError> {
        Self::load_from("config")
    }

    pub fn load_from(dir: &str) -> Result<Self, config::ConfigError> {
        let env = std::env::var("ANTHOVAI_ENV").unwrap_or_else(|_| "local".to_owned());
        config::Config::builder()
            .add_source(config::File::with_name(&format!("{dir}/default")))
            .add_source(config::File::with_name(&format!("{dir}/{env}")).required(false))
            .add_source(
                config::Environment::with_prefix("ANTHOVAI")
                    .separator("__")
                    .try_parsing(true)
                    // Without this a `Vec` field cannot come from the
                    // environment at all, and `dashboard_origins` is exactly
                    // the field a deployment needs to set: it is the origin
                    // allowlist every dashboard request is checked against, so
                    // a new domain would otherwise mean rebuilding the image.
                    .list_separator(",")
                    .with_list_parse_key("server.dashboard_origins"),
            )
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment has to be able to set the origin allowlist.
    ///
    /// It is a `Vec`, and `config`'s environment source will not build one
    /// without being told what separates the items — so before
    /// `list_separator` this variable was accepted and then ignored, leaving
    /// the compiled-in default in place. The symptom would have been every
    /// dashboard request from the real site refused with `origin_not_allowed`,
    /// with the setting plainly visible in the deployment's own configuration.
    ///
    /// Serialised: `std::env` is process-wide and these tests share it.
    #[test]
    fn the_origin_allowlist_can_come_from_the_environment() {
        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _held = GUARD.lock().unwrap_or_else(|e| e.into_inner());

        let key = "ANTHOVAI__SERVER__DASHBOARD_ORIGINS";
        // SAFETY: guarded above, and restored before the lock is released.
        unsafe {
            std::env::set_var(key, "https://www.anthovai.com,https://anthovai.com");
        }
        let settings = Settings::load_from("../../config");
        unsafe {
            std::env::remove_var(key);
        }

        let settings = settings.expect("configuration should load");
        assert_eq!(
            settings.server.dashboard_origins,
            vec![
                "https://www.anthovai.com".to_owned(),
                "https://anthovai.com".to_owned(),
            ],
            "one comma-separated value must become two origins, not one"
        );
    }
}
