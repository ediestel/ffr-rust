//! Best-effort update checker. Shells out to curl once per session and
//! compares the current version against the latest GitHub release tag.
//! Never blocks server startup.

use std::sync::OnceLock;

const REPO: &str = "eckhartd/ffr.nvim"; // TODO(Phase D2): replace once repo is published
const VERSION: &str = env!("CARGO_PKG_VERSION");

static NOTICE: OnceLock<String> = OnceLock::new();

pub fn get_update_notice() -> &'static str {
    NOTICE.get().map(|s| s.as_str()).unwrap_or("")
}

pub fn spawn_update_check() {
    std::thread::spawn(|| {
        let notice = fetch_latest_tag()
            .ok()
            .and_then(|tag| render_notice(&tag))
            .unwrap_or_default();
        let _ = NOTICE.set(notice);
    });
}

fn fetch_latest_tag() -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github.v3+json",
            &format!("https://api.github.com/repos/{REPO}/releases?per_page=1"),
        ])
        .output()?;
    if !output.status.success() {
        return Err("curl failed".into());
    }
    let body = String::from_utf8(output.stdout)?;
    // Minimal parse: find the first "tag_name":"..." value. Avoids adding a
    // JSON dep to the update-check path; body is tiny.
    if let Some(start) = body.find("\"tag_name\"") {
        if let Some(colon) = body[start..].find(':') {
            let after = &body[start + colon + 1..];
            if let Some(q1) = after.find('"') {
                let after = &after[q1 + 1..];
                if let Some(q2) = after.find('"') {
                    return Ok(after[..q2].trim().to_string());
                }
            }
        }
    }
    Err("tag_name not found".into())
}

fn render_notice(latest_tag: &str) -> Option<String> {
    let latest = latest_tag.trim_start_matches('v');
    if latest.is_empty() || latest == VERSION {
        return None;
    }
    Some(format!(
        "\n[ffr update available: {VERSION} → {latest} (see https://github.com/{REPO}/releases)]\n"
    ))
}
