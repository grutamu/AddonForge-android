//! CurseForge.
//!
//! We have no CurseForge API key (their keys can't ship in an open-source
//! app), so this goes around the API: cfwidget.com, a third-party service
//! that mirrors each project's public file list, says what the latest file
//! is, and the zip comes from mediafilez.forgecdn.net, CurseForge's file
//! storage, which still serves files without a key. Either can stop working
//! at any time; the addon then falls back to its CurseForge link.

use super::Remote;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Project {
    #[serde(default)]
    files: Vec<File>,
    #[serde(default)]
    summary: String,
    /// The project page as HTML.
    #[serde(default)]
    description: String,
    #[serde(default)]
    thumbnail: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct File {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    display: String,
    /// release | beta | alpha
    #[serde(default, rename = "type")]
    kind: String,
    /// Game versions, "1.60.1" for Forever.
    #[serde(default)]
    versions: Vec<String>,
}

pub async fn resolve(client: &reqwest::Client, id: u64, prerelease: bool) -> anyhow::Result<Remote> {
    let resp = client.get(format!("https://api.cfwidget.com/{id}")).send().await?;
    match resp.status().as_u16() {
        // cfwidget fetches projects it hasn't seen yet in the background.
        202 => anyhow::bail!("cfwidget is still fetching this CurseForge project, try again in a minute"),
        404 => anyhow::bail!("cfwidget doesn't know CurseForge project {id}"),
        _ => {}
    }
    let p: Project = resp.error_for_status()?.json().await?;
    let (f, forever) = pick(&p.files, prerelease).ok_or_else(|| anyhow::anyhow!("CurseForge lists no files for this addon"))?;
    if f.name.is_empty() {
        anyhow::bail!("CurseForge file {} has no file name", f.id);
    }
    Ok(Remote {
        version: if f.display.is_empty() { f.name.trim_end_matches(".zip").to_string() } else { f.display.clone() },
        download_url: cdn_url(f.id, &f.name),
        filename: f.name.clone(),
        forever: Some(forever),
        prerelease: f.kind != "release",
    })
}

/// The project's summary line, its description as plain text, and its
/// thumbnail, for the details pane.
pub async fn details(client: &reqwest::Client, id: u64) -> Option<(String, String, Option<String>)> {
    let p: Project = client.get(format!("https://api.cfwidget.com/{id}")).send().await.ok()?.error_for_status().ok()?.json().await.ok()?;
    Some((p.summary, html_text(&p.description), p.thumbnail))
}

/// Tags dropped, entities for the common characters decoded, whitespace collapsed.
fn html_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Newest Forever file: releases first, then betas, alpha only as a last
/// resort; with betas allowed a newer beta beats an older release. No
/// Forever file at all: the same pick over every flavour, flagged so the
/// install asks first.
fn pick(files: &[File], prerelease: bool) -> Option<(&File, bool)> {
    let newest = |forever: bool, kind: &str| {
        files
            .iter()
            .filter(|f| f.kind == kind && (!forever || f.versions.iter().any(|v| v.starts_with("1.60"))))
            .max_by_key(|f| f.id)
    };
    let best = |forever: bool| {
        let (rel, beta) = (newest(forever, "release"), newest(forever, "beta"));
        let stable_first = if prerelease { rel.into_iter().chain(beta).max_by_key(|f| f.id) } else { rel.or(beta) };
        stable_first.or_else(|| newest(forever, "alpha"))
    };
    best(true).map(|f| (f, true)).or_else(|| best(false).map(|f| (f, false)))
}

/// `files/{id / 1000}/{id % 1000}/{name}`; the last part loses leading zeros.
fn cdn_url(id: u64, name: &str) -> String {
    let enc: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://mediafilez.forgecdn.net/files/{}/{}/{enc}", id / 1000, id % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: u64, kind: &str, versions: &[&str]) -> File {
        File {
            id,
            name: format!("a-{id}.zip"),
            display: String::new(),
            kind: kind.into(),
            versions: versions.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn cdn_url_splits_the_id_and_encodes_the_name() {
        assert_eq!(cdn_url(9187447, "Details-1.0.zip"), "https://mediafilez.forgecdn.net/files/9187/447/Details-1.0.zip");
        assert_eq!(cdn_url(4515007, "a b+c.zip"), "https://mediafilez.forgecdn.net/files/4515/7/a%20b%2Bc.zip");
    }

    #[test]
    fn picks_newest_forever_release_before_betas() {
        let files = [f(1, "release", &["1.60.1"]), f(2, "release", &["1.15.9"]), f(3, "alpha", &["1.60.1"]), f(4, "beta", &["1.60.1"])];
        assert_eq!(pick(&files, false).map(|(f, fv)| (f.id, fv)), Some((1, true)));
        assert_eq!(pick(&files, true).map(|(f, fv)| (f.id, fv)), Some((4, true)));
    }

    #[test]
    fn alpha_only_when_nothing_else_and_flags_non_forever() {
        assert_eq!(pick(&[f(5, "alpha", &["1.60.1"])], false).map(|(f, _)| f.id), Some(5));
        assert_eq!(pick(&[f(6, "release", &["12.1.5"])], false).map(|(f, fv)| (f.id, fv)), Some((6, false)));
        assert!(pick(&[], false).is_none());
    }
}
