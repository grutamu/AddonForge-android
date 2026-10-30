use super::{Note, Remote};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TukAddon {
    #[serde(default)]
    version: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    patch: Vec<String>,
    #[serde(default)]
    changelog_url: String,
}

async fn addon(client: &reqwest::Client, slug: &str) -> anyhow::Result<TukAddon> {
    let url = format!("https://api.tukui.org/v1/addon/{slug}");
    let resp = client.get(&url).send().await?;
    if resp.status().as_u16() == 404 {
        anyhow::bail!("TukUI has no addon '{slug}'");
    }
    Ok(resp.error_for_status()?.json().await?)
}

pub async fn resolve(client: &reqwest::Client, slug: &str) -> anyhow::Result<Remote> {
    let a = addon(client, slug).await?;
    if a.url.is_empty() {
        anyhow::bail!("TukUI entry has no download url");
    }
    let forever = Some(a.patch.iter().any(|p| p.starts_with("1.60")));
    Ok(Remote {
        version: a.version.clone(),
        download_url: a.url,
        filename: format!("{slug}-{}.zip", a.version),
        forever,
        prerelease: false,
    })
}

pub async fn notes(client: &reqwest::Client, slug: &str) -> anyhow::Result<Vec<Note>> {
    let a = addon(client, slug).await?;
    if !a.changelog_url.starts_with("https://") {
        return Ok(Vec::new());
    }
    let text = client.get(&a.changelog_url).send().await?.error_for_status()?.text().await?;
    Ok(vec![Note { version: a.version, text }])
}
