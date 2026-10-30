use serde::{Deserialize, Serialize};

pub const ANDROID_RELEASES_API: &str = "https://api.github.com/repos/grutamu/AddonForge-android/releases/latest";

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 0 for local builds.
pub fn build() -> u32 {
    option_env!("ADDONFORGE_BUILD").and_then(|s| s.parse().ok()).unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Latest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub build: u32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SelfUpdate {
    pub current_version: String,
    pub current_build: u32,
    pub latest: Option<Latest>,
    pub available: bool,
    pub tag: String,
    pub error: Option<String>,
}

pub async fn check(client: &reqwest::Client, url_override: Option<&str>) -> SelfUpdate {
    let mut out = SelfUpdate {
        current_version: version().into(),
        current_build: build(),
        latest: None,
        available: false,
        tag: String::new(),
        error: None,
    };
    match android_latest(client, url_override.unwrap_or(ANDROID_RELEASES_API)).await {
        Ok(Some((l, newer))) => {
            out.available = newer;
            out.tag = format!("v{}-b{}", l.version, l.build);
            out.latest = Some(l);
        }
        Ok(None) => {}
        Err(e) => out.error = Some(e.to_string()),
    }
    out
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
}

/// `url` is the release page, not the APK.
async fn android_latest(client: &reqwest::Client, api: &str) -> anyhow::Result<Option<(Latest, bool)>> {
    let r: GhRelease = client
        .get(api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let Some(apk) = r.assets.iter().find(|a| a.name.to_ascii_lowercase().ends_with(".apk")) else {
        return Ok(None);
    };
    let Some((name, code)) = r.tag_name.strip_prefix("android-").and_then(|n| Some((n, release_code(n)?))) else {
        return Ok(None);
    };
    let l = Latest { version: name.into(), build: code, url: r.html_url, filename: apk.name.clone(), notes: r.body };
    let newer = release().and_then(release_code).is_some_and(|current| code > current);
    Ok(Some((l, newer)))
}

/// None for local builds.
pub fn release() -> Option<&'static str> {
    option_env!("ADDONFORGE_RELEASE").filter(|s| !s.is_empty())
}

/// `2026.10.14-2` → 2026101402. Must match android-release.yml.
pub fn release_code(name: &str) -> Option<u32> {
    let (date, n) = match name.split_once('-') {
        Some((d, n)) => (d, n.parse::<u32>().ok()?),
        None => (name, 1),
    };
    let mut parts = date.split('.').map(|p| p.parse::<u32>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || !(2000..2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) || !(1..=99).contains(&n) {
        return None;
    }
    Some((y * 10000 + m * 100 + d) * 100 + n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_release_names() {
        assert_eq!(release_code("2026.10.14"), Some(2026101401));
        assert_eq!(release_code("2026.10.14-2"), Some(2026101402));
        assert_eq!(release_code("2026.1.5"), Some(2026010501));
        assert!(release_code("2026.10.15").unwrap() > release_code("2026.10.14-9").unwrap());
        assert!(release_code("2026.11.1").unwrap() > release_code("2026.10.31").unwrap());
        for bad in ["v0.3.0-b2", "0.3.0", "2026.13.1", "2026.10.14-0", "2026.10.14.2", ""] {
            assert_eq!(release_code(bad), None, "{bad}");
        }
    }
}
