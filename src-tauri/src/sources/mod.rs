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

#[derive(Serialize, Clone, Debug)]
pub struct Remote {
    pub version: String,
    pub download_url: String,
    pub filename: String,
    /// None when the source doesn't say.
    pub forever: Option<bool>,
    pub prerelease: bool,
}

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

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub version: String,
    pub text: String,
}

const NOTE_MAX: usize = 2500;

/// Newest first. Single-changelog sources are cut at the installed version.
pub async fn release_notes(
    client: &reqwest::Client,
    src: &Source,
    installed: Option<&str>,
    opts: ResolveOpts<'_>,
) -> anyhow::Result<Vec<Note>> {
    let (notes, cumulative) = match src {
        Source::Github(repo) => (github::notes(client, repo, installed, opts.prerelease).await?, false),
        Source::Wago(id) => (wago::notes(client, id, opts.wago_key, opts.prerelease).await?, false),
        Source::Wowi(id) => (wowi::notes(client, *id).await?, true),
        Source::Tukui(slug) => (tukui::notes(client, slug).await?, true),
        Source::Curse(_) => (Vec::new(), false),
    };
    Ok(notes
        .into_iter()
        .map(|n| {
            let text = plain_notes(&n.text);
            let text = if cumulative { since(&text, installed) } else { text };
            Note { version: n.version, text: clip(&text, NOTE_MAX) }
        })
        .filter(|n| !n.text.is_empty())
        .collect())
}

/// The changelog above the line naming `installed`.
pub fn since(text: &str, installed: Option<&str>) -> String {
    let Some(v) = installed.filter(|v| !normalize_version(v).is_empty()) else { return text.to_string() };
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let v = [v.to_string()];
    match lines.iter().enumerate().skip(first + 1).find(|(_, l)| names_every_version(l, &v)) {
        Some((i, _)) => lines[..i].join("\n").trim_end().to_string(),
        None => text.to_string(),
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let end = text.char_indices().nth(max).map(|(i, _)| i).unwrap_or(text.len());
    let cut = text[..end].rfind('\n').filter(|&i| i > end / 2).unwrap_or(end);
    format!("{}\n…", text[..cut].trim_end())
}

/// Markdown, BBCode or HTML to plain text.
pub fn plain_notes(raw: &str) -> String {
    let s = raw.replace("\r\n", "\n").replace('\r', "\n");
    let s = strip_tags(&s, '[', ']', |name, closing| match name {
        "*" => Some("\n• "),
        "list" | "ul" | "ol" | "li" | "quote" | "code" | "center" | "indent" | "left" | "right" | "spoiler" => Some(if closing { "\n" } else { "" }),
        "b" | "i" | "u" | "s" | "size" | "color" | "font" | "url" | "email" | "highlight" | "h1" | "h2" | "h3" => Some(""),
        "img" | "youtube" => Some("\u{1}"),
        _ => None,
    });
    let s = drop_marked(&s);
    let s = strip_tags(&s, '<', '>', |name, closing| match name {
        "br" | "p" | "div" | "ul" | "ol" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "details" | "summary" => Some("\n"),
        "li" => Some(if closing { "" } else { "\n• " }),
        _ if name.starts_with('!') => Some(""),
        _ if name.chars().all(|c| c.is_ascii_alphanumeric()) && !name.is_empty() => Some(""),
        _ => None,
    });
    let s = s
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let mut out: Vec<String> = Vec::new();
    for line in s.lines() {
        let body = line.trim();
        let indent = (line.len() - line.trim_start().len()).min(6) / 2;
        let body = if body.starts_with('#') { body.trim_start_matches('#').trim_start() } else { body };
        let body = match body.get(..2) {
            Some("* " | "- " | "+ ") => format!("• {}", body[2..].trim_start()),
            _ => body.to_string(),
        };
        let body = md_inline(&body);
        if body.is_empty() && out.last().map(|l| l.is_empty()).unwrap_or(true) {
            continue;
        }
        out.push(format!("{}{}", " ".repeat(indent), body).trim_end().to_string());
    }
    out.join("\n").trim().to_string()
}

/// Replaces tags `keep` recognises; other brackets (`a < b`) stay.
fn strip_tags(s: &str, open: char, close: char, keep: impl Fn(&str, bool) -> Option<&'static str>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(open) {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let tag = after.find(close).filter(|&j| j <= 400).map(|j| &after[..j]);
        let replaced = tag.and_then(|t| {
            let closing = t.starts_with('/');
            let name = t.trim_start_matches('/');
            let name = name.split(|c: char| c == '=' || c.is_whitespace() || c == '/').next().unwrap_or("");
            let first_ok = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '*' || c == '!');
            first_ok.then(|| keep(&name.to_ascii_lowercase(), closing)).flatten().map(|r| (r, t.len()))
        });
        match replaced {
            Some((r, len)) => {
                out.push_str(r);
                rest = &after[len + 1..];
            }
            None => {
                out.push(open);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn drop_marked(s: &str) -> String {
    s.split('\u{1}').step_by(2).collect()
}

fn md_inline(s: &str) -> String {
    let s = s.replace("**", "").replace("__", "").replace('`', "");
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find('[') {
        let image = rest[..i].ends_with('!');
        let link = rest[i + 1..].find("](").and_then(|j| {
            let text = &rest[i + 1..i + 1 + j];
            let tail = &rest[i + 1 + j + 2..];
            tail.find(')').filter(|_| !text.contains('[')).map(|k| (text, i + 1 + j + 2 + k + 1))
        });
        match link {
            Some((text, end)) => {
                out.push_str(&rest[..if image { i - 1 } else { i }]);
                if !image {
                    out.push_str(text);
                }
                rest = &rest[end..];
            }
            None => {
                out.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// "v1.2.3" and "#1.2.3" -> "1.2.3"
pub fn normalize_version(v: &str) -> String {
    v.trim().trim_start_matches(['v', 'V', '#']).trim().to_ascii_lowercase()
}

/// Whole-version matches only: `1.2` isn't in `1.2.3`.
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

    #[test]
    fn release_notes_become_plain_text() {
        let wowi = "[size=\"6\"][url=\"https://github.com/W/W/tree/5.22.1\"]5.22.1[/url] (2026-10-10)[/size]\r\n[url=\"https://x/compare/5.22.0...5.22.1\"]Full Changelog[/url]\r\nNnoggie (2):\r\n[list][*]Update <b>updater</b>[*]Fix [b]this[/b][/list]\r\n[img]https://x/y.png[/img]Done";
        assert_eq!(plain_notes(wowi), "5.22.1 (2026-10-10)\nFull Changelog\nNnoggie (2):\n\n• Update updater\n• Fix this\n\nDone");
        let md = "## What's Changed\n\n\n*   **Fixed** the `thing` in [#12](https://github.com/a/b/pull/12) &amp; more\n  - nested\n![shot](https://x/y.png)\n### Version 16.00 [ October 13th 2026 ]\n- [x] a < b";
        assert_eq!(plain_notes(md), "What's Changed\n\n• Fixed the thing in #12 & more\n • nested\n\nVersion 16.00 [ October 13th 2026 ]\n• [x] a < b");
        assert_eq!(plain_notes("<details><summary>More</summary><ul><li>one</li><li>two</li></ul></details>"), "More\n\n• one\n• two");
    }

    #[test]
    fn cumulative_changelog_stops_at_the_installed_version() {
        let log = "### Version 16.00 [ October 13th 2026 ]\n• big pass\n\n### Version 15.10 [ September 2026 ]\n• older\n### Version 15.09\n• oldest";
        assert_eq!(since(log, Some("15.10")), "### Version 16.00 [ October 13th 2026 ]\n• big pass");
        assert_eq!(since(log, Some("v15.09")), "### Version 16.00 [ October 13th 2026 ]\n• big pass\n\n### Version 15.10 [ September 2026 ]\n• older");
        assert_eq!(since(log, Some("14.0")), log, "unknown version: the whole log");
        assert_eq!(since(log, None), log);
        assert_eq!(since(log, Some("16.00")), log, "the first line is the latest entry itself");
    }

    #[test]
    fn long_notes_are_clipped_at_a_line_end() {
        let text = (0..100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let c = clip(&text, 50);
        assert!(c.ends_with("\n…"), "{c}");
        assert!(c.len() < 60 && c.starts_with("line 0\nline 1"), "{c}");
        assert_eq!(clip("short", 50), "short");
        assert_eq!(clip("ééééééééé", 4), "éééé\n…", "cuts on a char boundary");
    }
}
