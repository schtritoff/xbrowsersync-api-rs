use serde::Deserialize;

pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULTS: &str = include_str!("../config/settings.default.json");

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LogFile {
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogStdout {
    pub level: String,
}

impl Default for LogStdout {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Log {
    pub file: LogFile,
    pub stdout: LogStdout,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Server {
    pub behind_proxy: bool,
    pub host: String,
    pub port: u16,
    pub relative_path: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            behind_proxy: false,
            host: "0.0.0.0".to_string(),
            port: 8080,
            relative_path: "/".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    pub allow_new_syncs: bool,
    pub message: String,
    pub online: bool,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            allow_new_syncs: true,
            message: String::new(),
            online: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub allowed_origins: Vec<String>,
    pub daily_new_syncs_limit: u32,
    pub db_path: String,
    pub location: String,
    pub log: Log,
    pub max_syncs: u32,
    pub max_sync_size: usize,
    pub purge_interval_seconds: u64,
    pub purge_stale_syncs_days: u32,
    pub server: Server,
    pub status: Status,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            daily_new_syncs_limit: 0,
            db_path: "data/xbrowsersync.sqlite3".to_string(),
            location: String::new(),
            log: Log::default(),
            max_syncs: 0,
            max_sync_size: 512_000,
            purge_interval_seconds: 60,
            purge_stale_syncs_days: 21,
            server: Server::default(),
            status: Status::default(),
        }
    }
}

/// Deep merge `other` over `value` (objects recurse, scalars/arrays overwrite).
fn merge_values(value: &mut serde_json::Value, other: serde_json::Value) {
    match (value, other) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(&k) {
                    Some(existing) => merge_values(existing, v),
                    None => {
                        a.insert(k, v);
                    }
                }
            }
        }
        (slot, incoming) => *slot = incoming,
    }
}

/// Coerce a raw env string by trust-boundary heuristics (bool / int / string).
fn guess_typed(raw: &str) -> serde_json::Value {
    let raw = raw.trim_matches('"');
    if raw == "true" {
        serde_json::Value::Bool(true)
    } else if raw == "false" {
        serde_json::Value::Bool(false)
    } else if let Ok(n) = raw.parse::<u64>() {
        serde_json::Value::from(n)
    } else {
        serde_json::Value::String(raw.to_string())
    }
}

fn set_path(value: &mut serde_json::Value, path: &str, v: serde_json::Value) {
    let mut segs: Vec<&str> = path.split('.').collect();
    let last = segs.pop().expect("non-empty path");
    let mut cur = value;
    for seg in segs {
        cur = cur
            .get_mut(seg)
            .expect("config path must exist in defaults");
    }
    cur[last] = v;
}

fn apply_env(value: &mut serde_json::Value) {
    use std::collections::BTreeMap;
    let map: BTreeMap<&str, &str> = BTreeMap::from([
        ("DB_PATH", "dbPath"),
        ("DAILYNEWSYNCSLIMIT", "dailyNewSyncsLimit"),
        ("MAXSYNCS", "maxSyncs"),
        ("MAXSYNCSIZE", "maxSyncSize"),
        ("PURGEINTERVALSECONDS", "purgeIntervalSeconds"),
        ("PURGESTALESYNCDAYS", "purgeStaleSyncsDays"),
        ("LOCATION", "location"),
        ("LOG_STDOUT_LEVEL", "log.stdout.level"),
        ("LOG_FILE_ENABLED", "log.file.enabled"),
        ("LOG_FILE_PATH", "log.file.path"),
        ("SERVER_HOST", "server.host"),
        ("SERVER_PORT", "server.port"),
        ("SERVER_RELATIVEPATH", "server.relativePath"),
        ("SERVER_BEHINDPROXY", "server.behindProxy"),
        ("STATUS_ONLINE", "status.online"),
        ("STATUS_ALLOWNEWSYNCS", "status.allowNewSyncs"),
        ("STATUS_MESSAGE", "status.message"),
        ("ALLOWEDORIGINS", "allowedOrigins"),
    ]);

    for (env_key, path) in &map {
        if let Ok(raw) = std::env::var(format!("XBSAPI_{env_key}")) {
            let parsed = if *env_key == "ALLOWEDORIGINS" {
                serde_json::Value::Array(
                    raw.split(',')
                        .map(|s| serde_json::Value::String(s.trim().to_string()))
                        .filter(|v| v.as_str() != Some(""))
                        .collect(),
                )
            } else {
                guess_typed(&raw)
            };
            set_path(value, path, parsed);
        }
    }
}

impl Config {
    /// Defaults -> config/settings.json -> XBSAPI_* env vars.
    pub fn load() -> Result<Self, String> {
        let mut value: serde_json::Value =
            serde_json::from_str(DEFAULTS).map_err(|e| format!("default settings: {e}"))?;

        if let Ok(user) = std::fs::read_to_string("config/settings.json") {
            if let Ok(user_value) = serde_json::from_str::<serde_json::Value>(&user) {
                merge_values(&mut value, user_value);
            }
        }

        apply_env(&mut value);

        serde_json::from_value(value).map_err(|e| format!("invalid settings: {e}"))
    }

    /// Normalized relative path: starts and ends with `/`.
    pub fn relative_path(&self) -> String {
        let mut p = self.server.relative_path.clone();
        if !p.starts_with('/') {
            p = format!("/{p}");
        }
        if !p.ends_with('/') {
            p.push('/');
        }
        p
    }
}

/// Strip `<script>...</script>` blocks from the status message, like upstream.
pub fn strip_scripts(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"<script") {
            if let Some(end) = lower[i..].find("</script>") {
                i += end + "</script>".len();
                continue;
            }
        }
        if let Some(c) = html[i..].chars().next() {
            out.push(c);
            i += c.len_utf8();
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_scripts_removes_script_blocks() {
        assert_eq!(
            strip_scripts("hi <script>alert(1)</script> world"),
            "hi  world"
        );
        assert_eq!(strip_scripts("<SCRIPT>x</SCRIPT>"), "");
        assert_eq!(strip_scripts("no scripts here"), "no scripts here");
        assert_eq!(strip_scripts(""), "");
        assert_eq!(strip_scripts("a<script"), "a<script");
    }

    #[test]
    fn config_defaults_parse() {
        let cfg = Config::load().unwrap();
        assert_eq!(cfg.max_sync_size, 512_000);
        assert!(cfg.status.online);
        assert!(cfg.status.allow_new_syncs);
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.relative_path(), "/");
    }

    #[test]
    fn relative_path_normalizes() {
        let mut cfg = Config::default();
        cfg.server.relative_path = "api".to_string();
        assert_eq!(cfg.relative_path(), "/api/");
    }

    #[test]
    fn merge_overrides_scalars() {
        let mut base = serde_json::json!({"a": 1, "nested": {"x": 1, "y": 2}});
        merge_values(&mut base, serde_json::json!({"nested": {"x": 9}}));
        assert_eq!(base["nested"]["x"], 9);
        assert_eq!(base["nested"]["y"], 2);
    }
}
