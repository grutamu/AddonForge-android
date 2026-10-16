//! Addon sources. Each resolves "what is the latest release and where is
//! the zip" for one addon. The app talks to these sites directly over
//! HTTPS; nothing goes through a Pocket Forge server.

pub mod curseforge;
pub mod github;
pub mod tukui;
pub mod wago;
pub mod wowi;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum Source {
    Github(String),
    Wago(String),
    Wowi(u64),
    Tukui(String),
    /// CurseForge project id.
    Curse(u64),
}

impl Source {
    pub fn kind(&self) -> &'static str {
        match self {
            Source::Github(_) => "github",
            Source::Wago(_) => "wago",
            Source::Wowi(_) => "wowi",
            Source::Tukui(_) => "tukui",
            Source::Curse(_) => "curse",
        }
    }
    pub fn id(&self) -> String {
        match self {
            Source::Github(r) => r.clone(),
            Source::Wago(i) => i.clone(),
            Source::Wowi(i) => i.to_string(),
            Source::Tukui(s) => s.clone(),
            Source::Curse(i) => i.to_string(),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Source::Github(r) => format!("GitHub · {r}"),
            Source::Wago(_) => "Wago".into(),
            Source::Wowi(_) => "WoWInterface".into(),
            Source::Tukui(_) => "TukUI".into(),
            Source::Curse(_) => "CurseForge".into(),
        }
    }
    pub fn url(&self) -> String {
        match self {
            Source::Github(r) => format!("https://github.com/{r}/releases/latest"),
            Source::Wago(i) => format!("https://addons.wago.io/addons/{i}"),
            Source::Wowi(i) => format!("https://www.wowinterface.com/downloads/info{i}"),
            Source::Tukui(s) => format!("https://tukui.org/{s}"),
            Source::Curse(i) => format!("https://www.curseforge.com/projects/{i}"),
        }
    }
    /// Rebuild from the strings stored in state.json.
    pub fn from_parts(kind: &str, id: &str) -> Option<Source> {
        match kind {
            "github" => Some(Source::Github(id.to_string())),
            "wago" => Some(Source::Wago(id.to_string())),
            "wowi" => id.parse().ok().map(Source::Wowi),
            "tukui" => Some(Source::Tukui(id.to_string())),
            "curse" => id.parse().ok().map(Source::Curse),
            _ => None,
        }
    }
}

/// The latest release as the source reports it.
#[derive(Serialize, Clone, Debug)]
pub struct Remote {
    pub version: String,
    pub download_url: String,
    pub filename: String,
    /// Some(true) = source says this build supports Forever,
    /// Some(false) = it says it doesn't, None = the source can't tell us.
    pub forever: Option<bool>,
    /// True when this came from a pre-release / beta channel.
    pub prerelease: bool,
}

/// Options that can influence resolution.
#[derive(Default, Clone, Debug)]
pub struct ResolveOpts<'a> {
    pub asset_hint: Option<&'a str>,
    pub wago_key: Option<&'a str>,
    pub prerelease: bool,
}

pub async fn resolve(
    client: &reqwest::Client,
    src: &Source,
    opts: ResolveOpts<'_>,
) -> anyhow::Result<Remote> {
    match src {
        Source::Github(repo) => github::resolve(client, repo, opts.asset_hint, opts.prerelease).await,
        Source::Wago(id) => wago::resolve(client, id, opts.wago_key, opts.prerelease).await,
        Source::Wowi(id) => wowi::resolve(client, *id).await,
        Source::Tukui(slug) => tukui::resolve(client, slug).await,
        Source::Curse(id) => curseforge::resolve(client, *id, opts.prerelease).await,
    }
}

/// "v1.2.3" == "1.2.3" for update checks. Some packaged TOCs say "#1.2.3".
pub fn normalize_version(v: &str) -> String {
    v.trim().trim_start_matches(['v', 'V', '#']).trim().to_ascii_lowercase()
}

/// Does the release name contain each of these versions as a whole version
/// (`12.1.0` in `Questie-v12.1.0+v1.0.6`, but `1.2` not in `1.2.3`)?
pub fn names_every_version(remote: &str, versions: &[String]) -> bool {
    let remote = remote.to_ascii_lowercase();
    let whole = |v: &str| {
        let v = normalize_version(v);
        !v.is_empty()
            && remote.match_indices(v.as_str()).any(|(i, _)| {
                let before = remote[..i].chars().last();
                let after = remote[i + v.len()..].chars().next();
                let edge = |c: Option<char>| c.map(|c| !c.is_ascii_alphanumeric() && c != '.').unwrap_or(true);
                (edge(before) || (before == Some('v') && edge(remote[..i - 1].chars().last()))) && edge(after)
            })
    };
    !versions.is_empty() && versions.iter().all(|v| whole(v))
}

/// Turn "https://github.com/owner/repo/..." or "owner/repo" into "owner/repo".
pub fn parse_github_repo(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('/');
    let s = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .or_else(|| s.strip_prefix("github.com/"))
        .or_else(|| s.strip_prefix("https://www.github.com/"))
        .unwrap_or(s);
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() < 2 {
        return None;
    }
    let ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    let (owner, repo) = (parts[0], parts[1].trim_end_matches(".git"));
    if ok(owner) && ok(repo) {
        Some(format!("{owner}/{repo}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_github_inputs() {
        assert_eq!(parse_github_repo("BigWigsMods/BigWigs").as_deref(), Some("BigWigsMods/BigWigs"));
        assert_eq!(parse_github_repo("https://github.com/BigWigsMods/BigWigs/releases/tag/v1").as_deref(), Some("BigWigsMods/BigWigs"));
        assert_eq!(parse_github_repo("github.com/a/b.git").as_deref(), Some("a/b"));
        assert_eq!(parse_github_repo("nope"), None);
        assert_eq!(parse_github_repo("https://gitlab.com/a/b"), None);
    }

    #[test]
    fn bundle_release_names_every_folder_version() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(names_every_version("Questie-v12.1.0+v1.0.6", &v(&["12.1.0", "1.0.6"])));
        assert!(!names_every_version("Questie-v12.1.0+v1.0.6", &v(&["12.0.4", "1.0.6"])), "old Questie, same database");
        assert!(!names_every_version("1.2.3", &v(&["1.2"])));
        assert!(!names_every_version("v11.2.3", &v(&["1.2.3"])));
        assert!(names_every_version("Leatrix_Plus-1.60.20-forever", &v(&["1.60.20"])));
        assert!(!names_every_version("anything", &[]));
    }
}
