//! Wago Addons (addons.wago.io).
//!
//! Wago's external API needs an API key. Keys are free from
//! https://addons.wago.io/account/apikeys and the user pastes their own into
//! Settings; it is stored locally and only ever sent to addons.wago.io.

use super::{Note, Remote};
use serde_json::Value;

pub const KEY_URL: &str = "https://addons.wago.io/account/apikeys";

/// The release we'd install and its channel (stable | beta | alpha).
async fn release(client: &reqwest::Client, id: &str, key: Option<&str>, prerelease: bool) -> anyhow::Result<(&'static str, Value)> {
    let key = key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Wago needs your API key (Settings)"))?;
    let url = format!("https://addons.wago.io/api/external/addons/{id}?game_version=forever");
    let resp = client.get(&url).bearer_auth(key).send().await?;
    match resp.status().as_u16() {
        401 | 403 => anyhow::bail!("Wago rejected the API key"),
        404 => anyhow::bail!("Wago has no Forever release for this addon"),
        _ => {}
    }
    let mut v: Value = resp.error_for_status()?.json().await?;
    let recent = v["recent_release"].take();
    // Stable first unless the user allows beta builds; alpha only as a last resort.
    let order: &[&'static str] = if prerelease { &["beta", "stable", "alpha"] } else { &["stable", "beta", "alpha"] };
    order
        .iter()
        .map(|s| (*s, &recent[*s]))
        .find(|(_, r)| r.is_object())
        .map(|(s, r)| (s, r.clone()))
        .ok_or_else(|| anyhow::anyhow!("Wago lists no release for Forever"))
}

pub async fn resolve(
    client: &reqwest::Client,
    id: &str,
    key: Option<&str>,
    prerelease: bool,
) -> anyhow::Result<Remote> {
    let (channel, rel) = release(client, id, key, prerelease).await?;
    let version = rel["label"]
        .as_str()
        .or_else(|| rel["version"].as_str())
        .unwrap_or("")
        .to_string();
    let download_url = rel["download_link"]
        .as_str()
        .or_else(|| rel["link"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Wago release has no download link"))?
        .to_string();
    let patch = rel["patch"].as_str().unwrap_or("");
    let forever = if patch.is_empty() { None } else { Some(patch.starts_with("1.60")) };
    let safe: String = version.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
    Ok(Remote {
        version,
        download_url,
        filename: format!("wago-{id}-{safe}.zip"),
        forever,
        prerelease: channel != "stable",
    })
}

/// The changelog of the release we'd install (Markdown).
pub async fn notes(client: &reqwest::Client, id: &str, key: Option<&str>, prerelease: bool) -> anyhow::Result<Vec<Note>> {
    let (_, rel) = release(client, id, key, prerelease).await?;
    let version = rel["label"].as_str().or_else(|| rel["version"].as_str()).unwrap_or("").to_string();
    Ok(rel["changelog"].as_str().map(|t| Note { version, text: t.to_string() }).into_iter().collect())
}
