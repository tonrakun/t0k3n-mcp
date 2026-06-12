use std::cmp::Ordering;
use std::sync::Arc;

use crate::dashboard::{DashboardState, UpdateInfo, UpdateKind};

const GITHUB_REPO: &str = "tonrakun/t0k3n-mcp";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn spawn_update_check(dashboard: Option<Arc<DashboardState>>) {
    tokio::spawn(async move {
        if let Err(e) = check_for_updates(dashboard).await {
            tracing::debug!("Update check skipped: {}", e);
        }
    });
}

/// Fetches the latest release version (without the leading `v`) from GitHub.
pub(crate) async fn fetch_latest_version() -> anyhow::Result<String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .user_agent(format!("t0k3n/{CURRENT_VERSION}"))
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let resp = client.get(&url).send().await?.error_for_status()?;
    let body = resp.text().await?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let tag = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v');

    if tag.is_empty() {
        anyhow::bail!("latest release has no tag_name");
    }
    Ok(tag.to_string())
}

async fn check_for_updates(dashboard: Option<Arc<DashboardState>>) -> anyhow::Result<()> {
    let tag = &fetch_latest_version().await?;

    let info = match compare_semver(CURRENT_VERSION, tag) {
        Ordering::Less => {
            tracing::info!(
                "⬆ Update available: v{CURRENT_VERSION} → v{tag} — run `t0k3n upgrade`"
            );
            UpdateInfo { available: Some(tag.to_string()), kind: UpdateKind::Available }
        }
        Ordering::Greater => {
            tracing::info!(
                "🧪 Beta Preview: running v{CURRENT_VERSION} (latest release: v{tag})"
            );
            UpdateInfo { available: Some(tag.to_string()), kind: UpdateKind::Beta }
        }
        Ordering::Equal => {
            tracing::debug!("t0k3n v{CURRENT_VERSION} is up to date");
            UpdateInfo { available: None, kind: UpdateKind::UpToDate }
        }
    };

    if let Some(d) = dashboard {
        d.set_update_info(info).await;
    }

    Ok(())
}

/// Compares two version strings of the form "MAJOR.MINOR.PATCH".
/// Returns `a.cmp(b)` in semver order.
pub(crate) fn compare_semver(a: &str, b: &str) -> Ordering {
    let parse_parts = |v: &str| -> Vec<u64> {
        // Strip any pre-release suffix (e.g. "1.2.0-beta.1" → [1, 2, 0])
        let base = v.split('-').next().unwrap_or(v);
        base.split('.').map(|s| s.parse().unwrap_or(0)).collect()
    };

    let va = parse_parts(a);
    let vb = parse_parts(b);
    let len = va.len().max(vb.len());

    for i in 0..len {
        let pa = va.get(i).copied().unwrap_or(0);
        let pb = vb.get(i).copied().unwrap_or(0);
        match pa.cmp(&pb) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_semver() {
        assert_eq!(compare_semver("1.1.0", "1.2.0"), Ordering::Less);
        assert_eq!(compare_semver("1.2.0", "1.1.0"), Ordering::Greater);
        assert_eq!(compare_semver("1.1.0", "1.1.0"), Ordering::Equal);
        assert_eq!(compare_semver("2.0.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_semver("1.1.0-beta.1", "1.1.0"), Ordering::Equal);
        assert_eq!(compare_semver("1.1.0", "1.1.1"), Ordering::Less);
    }
}
