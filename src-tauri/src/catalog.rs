//! The addon catalogue: built from instawow's daily catalogue
//! (github.com/layday/instawow-data), filtered to what CurseForge tags as
//! Forever.
//!
//! It is a *pointer list*, not a host: every entry says where the author
//! publishes releases. Downloads always come straight from that source.
//! The converted list is cached beside state.json and refreshed once a day.

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const REMOTE_URL: &str = "https://raw.githubusercontent.com/layday/instawow-data/data/base-catalogue-v8.compact.json";
const MAX_AGE: Duration = Duration::from_secs(24 * 3600);

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub category: String,
    /// `owner/repo` on GitHub. Releases are read from
    /// `https://github.com/<owner>/<repo>/releases/latest/download/release.json`.
    #[serde(default)]
    pub github: Option<String>,
    /// Wago Addons project id (needs the user's own free API key).
    #[serde(default)]
    pub wago: Option<String>,
    /// WoWInterface file id.
    #[serde(default)]
    pub wowi: Option<u64>,
    /// TukUI slug (elvui / tukui).
    #[serde(default)]
    pub tukui: Option<String>,
    /// CurseForge project id.
    #[serde(default)]
    pub curse: Option<u64>,
    /// Substring to pick the right zip when a GitHub release has several
    /// and no release.json.
    #[serde(default)]
    pub asset_hint: Option<String>,
    /// Top-level AddOns folders the release installs (used to match what's
    /// already on disk to this entry).
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Does the author publish a Forever build?  true / false / null=unknown.
    #[serde(default)]
    pub forever: Option<bool>,
    /// A short line shown on the installed row, e.g. "beta workaround, remove
    /// once Blizzard fixes X". Changing it is a catalogue edit, no release needed.
    #[serde(default)]
    pub notice: Option<String>,
    /// Date the entry was added (YYYY-MM-DD). Feeds "New in the catalogue".
    #[serde(default)]
    pub added: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Catalog {
    #[serde(default)]
    pub flavor: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub addons: Vec<CatalogEntry>,
    /// Downloads and last update per entry id, from the same source.
    #[serde(default)]
    pub stats: Stats,
}

/// Popularity per catalogue entry: download totals and last-release dates.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StatEntry {
    #[serde(default)]
    pub downloads: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// "curse" | "github" | "wowi": what the number counts.
    #[serde(default)]
    pub source: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Stats {
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub entries: std::collections::HashMap<String, StatEntry>,
}

#[derive(Deserialize)]
struct IwCatalogue {
    entries: Vec<IwEntry>,
}

#[derive(Deserialize)]
struct IwEntry {
    source: String,
    id: String,
    slug: String,
    name: String,
    #[serde(default)]
    url: String,
    /// One folder list per recent file.
    #[serde(default)]
    folders: Vec<Vec<String>>,
    #[serde(default)]
    same_as: Vec<IwRef>,
    #[serde(default)]
    game_flavours: Vec<String>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    last_updated: Option<String>,
}

#[derive(Deserialize)]
struct IwRef {
    source: String,
    id: String,
}

/// instawow's catalogue -> ours: every CurseForge entry tagged Forever, with
/// the GitHub repo that names it in `same_as` when exactly one does (the
/// repo is then the preferred, official source).
fn convert(iw: IwCatalogue) -> Catalog {
    let mut github_for: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in iw.entries.iter().filter(|e| e.source == "github") {
        for s in e.same_as.iter().filter(|s| s.source == "curse") {
            // Case-correct owner/repo from the URL, the slug is lowercased.
            let repo = e.url.strip_prefix("https://github.com/").unwrap_or(&e.slug);
            github_for.entry(s.id.as_str()).or_default().push(repo);
        }
    }
    let mut cat = Catalog { flavor: "forever".into(), ..Default::default() };
    let mut forever: Vec<&IwEntry> = iw
        .entries
        .iter()
        .filter(|e| e.source == "curse" && e.game_flavours.iter().any(|f| f == "forever_classic"))
        .collect();
    // Most downloaded first: the order Find addons starts in.
    forever.sort_by(|a, b| b.download_count.cmp(&a.download_count));
    for e in forever {
        let Ok(curse) = e.id.parse::<u64>() else { continue };
        let mut seen = HashSet::new();
        let folders = e.folders.iter().flatten().filter(|f| seen.insert(f.to_ascii_lowercase())).cloned().collect();
        let id = format!("curse-{curse}");
        if let Some(d) = &e.last_updated {
            cat.updated = cat.updated.clone().max(d.chars().take(10).collect());
        }
        cat.stats.entries.insert(
            id.clone(),
            StatEntry { downloads: Some(e.download_count), updated_at: e.last_updated.clone(), source: "curse".into() },
        );
        cat.addons.push(CatalogEntry {
            id,
            name: e.name.clone(),
            curse: Some(curse),
            github: github_for.get(e.id.as_str()).filter(|r| r.len() == 1).map(|r| r[0].to_string()),
            folders,
            url: Some(e.url.clone()).filter(|u| !u.is_empty()),
            forever: Some(true),
            ..Default::default()
        });
    }
    cat.stats.updated = cat.updated.clone();
    cat
}

fn cache_path() -> std::path::PathBuf {
    AppState::dir().join("catalogue.json")
}

fn read_cache() -> Option<(Catalog, bool)> {
    let path = cache_path();
    let age = std::fs::metadata(&path).ok()?.modified().ok()?.elapsed().unwrap_or_default();
    let cat: Catalog = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
    (!cat.addons.is_empty()).then_some((cat, age < MAX_AGE))
}

async fn fetch(client: &reqwest::Client) -> anyhow::Result<Catalog> {
    // About 4 MB for every flavour; filtered down to Forever right away.
    let bytes = client.get(REMOTE_URL).timeout(Duration::from_secs(60)).send().await?.error_for_status()?.bytes().await?;
    let cat = tokio::task::spawn_blocking(move || serde_json::from_slice::<IwCatalogue>(&bytes).map(convert)).await??;
    if cat.addons.is_empty() {
        anyhow::bail!("instawow's catalogue has no Forever entries");
    }
    let tmp = cache_path().with_extension("json.tmp");
    std::fs::create_dir_all(AppState::dir())?;
    std::fs::write(&tmp, serde_json::to_vec(&cat)?)?;
    std::fs::rename(&tmp, cache_path())?;
    Ok(cat)
}

/// Today's cached copy, else a fresh download, else an older cached copy;
/// with no copy at all, an empty catalogue.
pub async fn load(client: &reqwest::Client, refresh: bool) -> (Catalog, &'static str) {
    let cached = read_cache();
    if let Some((c, true)) = &cached {
        if !refresh {
            return (c.clone(), "cached");
        }
    }
    match fetch(client).await {
        Ok(c) => (c, "remote"),
        Err(e) => {
            crate::loge!("catalogue download: {e}");
            match cached {
                Some((c, _)) => (c, "cached (old)"),
                None => (Catalog::default(), "unavailable"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_forever_curse_entries_and_links_unique_github_repos() {
        let iw: IwCatalogue = serde_json::from_str(r#"{"entries":[
            {"source":"curse","id":"61284","slug":"details","name":"Details!","url":"https://www.curseforge.com/wow/addons/details","folders":[["Details","Details_Streamer"],["Details","Details_Vanguard"]],"game_flavours":["mainline","forever_classic"],"download_count":5,"last_updated":"2026-10-14T00:00:00+00:00"},
            {"source":"curse","id":"2","slug":"retail-only","name":"R","folders":[["R"]],"game_flavours":["mainline"],"download_count":9},
            {"source":"curse","id":"3","slug":"two-repos","name":"T","folders":[["T"]],"game_flavours":["forever_classic"],"download_count":7},
            {"source":"github","id":"1","slug":"tercioo/details","name":"Details","url":"https://github.com/Tercioo/Details","same_as":[{"source":"curse","id":"61284"}],"game_flavours":["mainline"]},
            {"source":"github","id":"8","slug":"a/t","name":"T","url":"https://github.com/a/t","same_as":[{"source":"curse","id":"3"}]},
            {"source":"github","id":"9","slug":"fork/t","name":"T","url":"https://github.com/fork/t","same_as":[{"source":"curse","id":"3"}]}
        ]}"#).unwrap();
        let cat = convert(iw);
        let ids: Vec<&str> = cat.addons.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["curse-3", "curse-61284"]);
        let d = &cat.addons[1];
        assert_eq!(d.folders, ["Details", "Details_Streamer", "Details_Vanguard"]);
        assert_eq!(d.github.as_deref(), Some("Tercioo/Details"));
        assert_eq!(cat.addons[0].github, None, "two repos claim it: no GitHub link");
        assert_eq!(cat.stats.entries["curse-61284"].downloads, Some(5));
        assert_eq!(cat.updated, "2026-10-14");
    }
}
