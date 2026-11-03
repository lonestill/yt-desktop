use serde::{Deserialize, Serialize};
use std::process::Command;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "lonestill/yt-desktop";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub release_url: String,
    pub published_at: Option<String>,
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let clean = v.trim().trim_start_matches('v');
    let mut parts = clean.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

pub fn check_for_updates() -> Option<UpdateInfo> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-H",
        "User-Agent: yt-desktop",
        "--connect-timeout",
        "4",
        "--max-time",
        "6",
        &url,
    ]);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let val: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let tag = val.get("tag_name")?.as_str()?;
    let html_url = val.get("html_url")?.as_str()?.to_string();
    let name = val.get("name").and_then(|n| n.as_str()).unwrap_or(tag).to_string();
    let published_at = val.get("published_at").and_then(|p| p.as_str()).map(|s| s.to_string());

    let remote = parse_version(tag)?;
    let current = parse_version(CURRENT_VERSION)?;

    if remote > current {
        Some(UpdateInfo {
            current_version: CURRENT_VERSION.to_string(),
            latest_version: tag.to_string(),
            release_name: name,
            release_url: html_url,
            published_at,
        })
    } else {
        None
    }
}

pub fn open_url_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", "start", "", url]);
        cmd.creation_flags(0x08000000);
        let _ = cmd.spawn();
    }

    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();
}
