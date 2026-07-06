//! Non-blocking new-version hint.
//!
//! Each run reads a small cache written by previous runs and prints at most
//! one stderr hint when a newer release is known. When the cache is older
//! than a day a background task refreshes it from the GitHub releases API;
//! the command itself never waits on the network beyond a short grace at
//! exit. Opt out with NOLGIA_NO_UPDATE_CHECK=1. Suppressed for agent
//! traffic (NOLGIA_SURFACE), CI, non-interactive stderr, and --json runs.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/nolgiainc/nolgia-cli/releases/latest";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);
const EXIT_GRACE: Duration = Duration::from_millis(400);
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct CheckCache {
    checked_at: DateTime<Utc>,
    latest: String,
}

#[derive(Deserialize)]
struct InstallMetadata {
    method: String,
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

pub struct UpdateCheck {
    hint: Option<String>,
    refresh: Option<tokio::task::JoinHandle<()>>,
}

pub fn start(json_output: bool) -> UpdateCheck {
    if json_output || disabled() {
        return UpdateCheck {
            hint: None,
            refresh: None,
        };
    }
    let cache = read_cache();
    let hint = cache
        .as_ref()
        .filter(|c| is_newer(&c.latest, CURRENT_VERSION))
        .map(|c| hint_line(&c.latest, &upgrade_command()));
    let refresh = match &cache {
        Some(c)
            if Utc::now()
                .signed_duration_since(c.checked_at)
                .to_std()
                .map(|d| d < CHECK_INTERVAL)
                .unwrap_or(true) =>
        {
            None
        }
        _ => Some(tokio::spawn(refresh_cache())),
    };
    UpdateCheck { hint, refresh }
}

impl UpdateCheck {
    pub async fn finish(self) {
        if let Some(handle) = self.refresh {
            let _ = tokio::time::timeout(EXIT_GRACE, handle).await;
        }
        if let Some(hint) = self.hint {
            eprintln!("{hint}");
        }
    }
}

fn disabled() -> bool {
    let set = |name: &str| std::env::var(name).is_ok_and(|v| !v.is_empty());
    set("NOLGIA_NO_UPDATE_CHECK")
        || set("CI")
        || set("NOLGIA_SURFACE")
        || !std::io::stderr().is_terminal()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn nolgia_dir(env_override: &str, default_suffix: &str) -> Option<PathBuf> {
    let base = match std::env::var_os(env_override) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home_dir()?.join(default_suffix),
    };
    Some(base.join("nolgia"))
}

fn cache_path() -> Option<PathBuf> {
    Some(nolgia_dir("XDG_STATE_HOME", ".local/state")?.join("update-check.json"))
}

fn metadata_path() -> Option<PathBuf> {
    Some(nolgia_dir("XDG_CONFIG_HOME", ".config")?.join("install-metadata.json"))
}

fn read_cache() -> Option<CheckCache> {
    let raw = std::fs::read_to_string(cache_path()?).ok()?;
    serde_json::from_str(&raw).ok()
}

async fn refresh_cache() {
    let Ok(client) = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build() else {
        return;
    };
    let Ok(response) = client
        .get(RELEASES_LATEST_URL)
        .header("User-Agent", format!("nolgia-cli/{CURRENT_VERSION}"))
        .send()
        .await
    else {
        return;
    };
    let Ok(release) = response.json::<LatestRelease>().await else {
        return;
    };
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let cache = CheckCache {
        checked_at: Utc::now(),
        latest,
    };
    let Some(path) = cache_path() else { return };
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(body) = serde_json::to_vec(&cache) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Numeric x.y.z comparison; anything unparseable is never "newer".
fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().trim_start_matches('v').splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

fn install_method() -> Option<String> {
    let raw = std::fs::read_to_string(metadata_path()?).ok()?;
    let metadata: InstallMetadata = serde_json::from_str(&raw).ok()?;
    Some(metadata.method)
}

fn upgrade_command() -> String {
    upgrade_command_for(
        install_method().as_deref(),
        &std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy(),
    )
}

fn upgrade_command_for(method: Option<&str>, exe_path: &str) -> String {
    match method {
        Some("npm") => "npm update -g @nolgia/cli".to_string(),
        Some("install.sh") => {
            "curl -fsSL https://raw.githubusercontent.com/nolgiainc/nolgia-cli/main/install.sh | bash".to_string()
        }
        _ if exe_path.contains(".cargo/bin") => "cargo install nolgia-cli".to_string(),
        _ if exe_path.contains("Cellar") || exe_path.contains("homebrew") => {
            "brew upgrade nolgia".to_string()
        }
        _ => "curl -fsSL https://raw.githubusercontent.com/nolgiainc/nolgia-cli/main/install.sh | bash".to_string(),
    }
}

fn hint_line(latest: &str, upgrade: &str) -> String {
    format!("nolgia {latest} is available (you have {CURRENT_VERSION}); upgrade with: {upgrade}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_compare_numerically() {
        assert!(is_newer("0.2.10", "0.2.9"));
        assert!(is_newer("v0.3.0", "0.2.1"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.1"));
    }

    #[test]
    fn upgrade_command_prefers_recorded_method() {
        assert_eq!(
            upgrade_command_for(Some("npm"), "/anywhere/nolgia"),
            "npm update -g @nolgia/cli"
        );
        assert!(upgrade_command_for(Some("install.sh"), "/anywhere/nolgia").contains("install.sh"));
    }

    #[test]
    fn upgrade_command_falls_back_to_exe_path() {
        assert_eq!(
            upgrade_command_for(None, "/Users/dev/.cargo/bin/nolgia"),
            "cargo install nolgia-cli"
        );
        assert_eq!(
            upgrade_command_for(None, "/opt/homebrew/Cellar/nolgia/0.2.1/bin/nolgia"),
            "brew upgrade nolgia"
        );
        assert!(upgrade_command_for(None, "/usr/local/bin/nolgia").contains("install.sh"));
    }
}
