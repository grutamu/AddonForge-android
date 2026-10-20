//! WoWInterface (wowinterface.com) public JSON API. No key needed.

use super::{Note, Remote};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct FileDetails {
    #[serde(default, rename = "UIVersion")]
    version: String,
    #[serde(default, rename = "UIDownload")]
    download: String,
    #[serde(default, rename = "UIFileName")]
    file_name: String,
    #[serde(default, rename = "UICompatibility")]
    compat: Option<Vec<Compat>>,
    /// Every version's notes, newest first (BBCode).
    #[serde(default, rename = "UIChangeLog")]
    changelog: String,
}

#[derive(Deserialize, Debug)]
struct Compat {
    #[serde(default)]
    version: String,
}

async fn details(client: &reqwest::Client, id: u64) -> anyhow::Result<FileDetails> {
    let url = format!("https://api.mmoui.com/v3/game/WOW/filedetails/{id}.json");
    let resp = client.get(&url).send().await?;
    if resp.status().as_u16() == 404 {
        anyhow::bail!("WoWInterface has no addon with id {id}");
    }
    let list: Vec<FileDetails> = resp.error_for_status()?.json().await?;
    list.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("WoWInterface returned nothing for {id}"))
}

pub async fn resolve(client: &reqwest::Client, id: u64) -> anyhow::Result<Remote> {
    let d = details(client, id).await?;
    if d.download.is_empty() {
        anyhow::bail!("WoWInterface entry has no download link");
    }
    let forever = d
        .compat
        .as_ref()
        .map(|c| c.iter().any(|x| x.version.starts_with("1.60")));
    let filename = if d.file_name.is_empty() {
        format!("wowi-{id}.zip")
    } else {
        d.file_name.clone()
    };
    Ok(Remote {
        version: d.version,
        download_url: d.download,
        filename,
        forever,
        prerelease: false,
    })
}

/// The addon's changelog: every version, newest first.
pub async fn notes(client: &reqwest::Client, id: u64) -> anyhow::Result<Vec<Note>> {
    let d = details(client, id).await?;
    Ok(vec![Note { version: d.version, text: d.changelog }])
}
