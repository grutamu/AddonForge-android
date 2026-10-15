//! AddonForge core: Tauri commands the UI calls over IPC.

mod catalog;
mod icons;
mod install;
pub mod log;
mod selfupdate;
mod sources;
mod state;
mod wow;

use catalog::{Catalog, CatalogEntry, Stats};
use serde::{Deserialize, Serialize};
use sources::{normalize_version, ResolveOpts, Source};
use state::{AppState, Installed};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use tokio::sync::{Mutex, Semaphore};
use wow::{AddonFolder, WowInstall};

/// What the addon sites see: this fork, so its traffic isn't taken for upstream's.
pub const USER_AGENT: &str = concat!(
    "AddonForge-android/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/grutamu/AddonForge-android)"
);
/// Where "Report on GitHub" files issues: this fork, not upstream.
pub const REPO_URL: &str = "https://github.com/grutamu/AddonForge-android";

struct App {
    client: reqwest::Client,
    state: Mutex<AppState>,
    catalog: Mutex<Option<(Catalog, &'static str)>>,
    /// icon key -> data URL, for this run (disk cache underneath).
    icon_cache: Mutex<HashMap<String, Option<String>>>,
}

type Shared = Arc<App>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------------------------------------------------------------- packages

#[derive(Serialize, Clone, Debug)]
pub struct MissingDep {
    pub folder: String,
    pub catalog_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Package {
    pub key: String,
    pub name: String,
    pub folders: Vec<String>,
    pub installed_version: Option<String>,
    pub author: Option<String>,
    pub notes: Option<String>,
    pub source: Option<Source>,
    pub source_label: String,
    pub source_url: Option<String>,
    pub curse_id: Option<u64>,
    pub catalog_id: Option<String>,
    /// Catalogue `notice` for this addon, shown on its row.
    pub notice: Option<String>,
    pub supports_forever: bool,
    /// True when AddonForge installed this itself (exact version known).
    pub managed: bool,
    pub pinned: bool,
    pub ignored: bool,
    pub missing_deps: Vec<MissingDep>,
    /// `## IconTexture` of the first folder that has one (resolved lazily by `package_icons`).
    pub icon_texture: Option<String>,
    pub remote_version: Option<String>,
    pub remote_forever: Option<bool>,
    pub remote_prerelease: bool,
    pub update_available: bool,
    /// ok | update | unknown | curse-only | no-source | no-key | error | checking
    pub status: String,
    pub note: String,
}

fn addons_dir(install: &str) -> PathBuf {
    Path::new(install).join("Interface").join("AddOns")
}

fn pick_source(
    entry: Option<&CatalogEntry>,
    f: &AddonFolder,
    has_wago: bool,
    curse: bool,
    managed: Option<&Installed>,
) -> Option<Source> {
    // What we installed it from wins: it's what the user chose. CurseForge
    // only while the unofficial switch stays on.
    if let Some(m) = managed {
        if let Some(s) = Source::from_parts(&m.source, &m.source_id).filter(|s| curse || !matches!(s, Source::Curse(_))) {
            return Some(s);
        }
    }
    if let Some(e) = entry {
        if let Some(r) = &e.github {
            return Some(Source::Github(r.clone()));
        }
        if has_wago {
            if let Some(w) = &e.wago {
                return Some(Source::Wago(w.clone()));
            }
        }
        if let Some(i) = e.wowi {
            return Some(Source::Wowi(i));
        }
        if let Some(t) = &e.tukui {
            return Some(Source::Tukui(t.clone()));
        }
    }
    // The addon's own files can name an official source the catalogue doesn't.
    if let Some(w) = f.wago.as_ref().filter(|_| has_wago) {
        return Some(Source::Wago(w.clone()));
    }
    if let Some(i) = f.wowi {
        return Some(Source::Wowi(i));
    }
    if let Some(repo) = f.website.as_deref().and_then(sources::parse_github_repo) {
        return Some(Source::Github(repo));
    }
    // CurseForge is the last resort, but beats a Wago entry we have no key for.
    if let Some(c) = entry.and_then(|e| e.curse).or(f.curse).filter(|_| curse) {
        return Some(Source::Curse(c));
    }
    // Listed but user has no key: surface it so the UI can explain.
    entry.and_then(|e| e.wago.clone()).or_else(|| f.wago.clone()).map(Source::Wago)
}

/// Lowercase letters and digits only: "DBM - Deadly Boss Mods (DBM-Core)" -> "dbmdeadlybossmodsdbmcore".
fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

/// Does the entry's name contain this folder's name (`!BugGrabber` in "BugGrabber")?
fn named_after(e: &CatalogEntry, folder: &str) -> bool {
    let f = squash(folder);
    !f.is_empty() && squash(&e.name).contains(&f)
}

/// Group scanned folders into packages and attach sources.
fn build_packages(st: &AppState, cat: &Catalog, folders: &[AddonFolder]) -> Vec<Package> {
    let mut by_folder: HashMap<String, &CatalogEntry> = HashMap::new();
    let mut by_wago: HashMap<&str, &CatalogEntry> = HashMap::new();
    let mut by_curse: HashMap<u64, &CatalogEntry> = HashMap::new();
    let mut by_wowi: HashMap<u64, &CatalogEntry> = HashMap::new();
    for e in &cat.addons {
        for f in &e.folders {
            // Several entries can ship the same folder (a library, a module, a
            // repack): the one named after it wins, then the smallest. Big
            // bundles never win a folder by name alone.
            if e.folders.len() > 30 && !named_after(e, f) {
                continue;
            }
            let rank = |x: &CatalogEntry| (!named_after(x, f), x.folders.len());
            let slot = by_folder.entry(f.to_ascii_lowercase()).or_insert(e);
            if rank(e) < rank(slot) {
                *slot = e;
            }
        }
        if let Some(w) = &e.wago {
            by_wago.insert(w.as_str(), e);
        }
        if let Some(c) = e.curse {
            by_curse.insert(c, e);
        }
        if let Some(i) = e.wowi {
            by_wowi.insert(i, e);
        }
    }
    let has_wago = st.has_wago_key();
    let curse = st.curse_unofficial;

    // Folders we installed ourselves are authoritative: they stay with their package.
    let mut managed_folder: HashMap<String, &str> = HashMap::new();
    for (key, rec) in &st.installed {
        for f in &rec.folders {
            managed_folder.insert(f.to_ascii_lowercase(), key.as_str());
        }
    }
    let on_disk: HashMap<String, &AddonFolder> =
        folders.iter().map(|f| (f.folder.to_ascii_lowercase(), f)).collect();

    // First pass: direct keys (managed / catalogue / ids / standalone).
    let direct_key = |f: &AddonFolder| -> (String, Option<&CatalogEntry>) {
        // The folder's own ids are exact; the folder name is a guess, and
        // when the ids name something else only a close guess counts (an
        // installed WeakAuras is not a repack that ships its folders).
        let by_id = f
            .curse
            .and_then(|c| by_curse.get(&c).copied())
            .or_else(|| f.wago.as_deref().and_then(|w| by_wago.get(w).copied()))
            .or_else(|| f.wowi.and_then(|i| by_wowi.get(&i).copied()));
        let has_ids = f.curse.is_some() || f.wago.is_some() || f.wowi.is_some();
        let by_name = by_folder
            .get(&f.folder.to_ascii_lowercase())
            .copied()
            .filter(|e| !has_ids || e.curse.is_none() || named_after(e, &f.folder));
        let cat_entry = by_id.or(by_name);
        if let Some(k) = managed_folder.get(&f.folder.to_ascii_lowercase()) {
            let entry = k
                .strip_prefix("cat:")
                .and_then(|id| cat.addons.iter().find(|e| e.id == id))
                .or(cat_entry);
            return ((*k).to_string(), entry);
        }
        let key = if let Some(e) = cat_entry {
            format!("cat:{}", e.id)
        } else if let Some(w) = &f.wago {
            format!("wago:{w}")
        } else if let Some(i) = f.wowi {
            format!("wowi:{i}")
        } else if let Some(c) = f.curse {
            format!("curse:{c}")
        } else {
            format!("folder:{}", f.folder)
        };
        (key, cat_entry)
    };

    // Does `f` look like a module of `parent` (e.g. BigWigs_Sporefall -> BigWigs,
    // DBM-GUI -> DBM-Core)? Needs a declared dependency and a shared name prefix.
    fn module_of<'a>(f: &AddonFolder, on_disk: &HashMap<String, &'a AddonFolder>) -> Option<&'a AddonFolder> {
        if let Some(p) = &f.part_of {
            if let Some(parent) = on_disk.get(&p.to_ascii_lowercase()) {
                return Some(parent);
            }
        }
        let stem = |s: &str| s.split(['_', '-']).next().unwrap_or(s).to_ascii_lowercase();
        let my_stem = stem(&f.folder);
        for d in &f.dependencies {
            if let Some(parent) = on_disk.get(&d.to_ascii_lowercase()) {
                if parent.folder != f.folder && stem(&parent.folder) == my_stem {
                    return Some(parent);
                }
            }
        }
        None
    }

    let mut keyed: Vec<(String, Option<&CatalogEntry>, &AddonFolder)> = Vec::with_capacity(folders.len());
    for f in folders {
        let (k, e) = direct_key(f);
        keyed.push((k, e, f));
    }
    // Second pass: standalone folders that are modules of another addon inherit its key.
    let key_of: HashMap<String, (String, Option<&CatalogEntry>)> = keyed
        .iter()
        .map(|(k, e, f)| (f.folder.to_ascii_lowercase(), (k.clone(), *e)))
        .collect();
    for item in keyed.iter_mut() {
        if !item.0.starts_with("folder:") {
            continue;
        }
        let mut cur = item.2;
        for _ in 0..4 {
            match module_of(cur, &on_disk) {
                Some(parent) => {
                    if let Some((k, e)) = key_of.get(&parent.folder.to_ascii_lowercase()) {
                        item.0 = k.clone();
                        item.1 = *e;
                    }
                    cur = parent;
                }
                None => break,
            }
        }
    }

    // key -> (entry, folders)
    let mut groups: Vec<(String, Option<&CatalogEntry>, Vec<&AddonFolder>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (key, entry, f) in keyed {
        match index.get(&key) {
            Some(&i) => {
                groups[i].2.push(f);
                if groups[i].1.is_none() {
                    groups[i].1 = entry;
                }
            }
            None => {
                index.insert(key.clone(), groups.len());
                groups.push((key, entry, vec![f]));
            }
        }
    }

    let mut out = Vec::new();
    for (key, entry, fs) in groups {
        let in_group: Vec<String> = fs.iter().map(|f| f.folder.to_ascii_lowercase()).collect();
        let is_root = |f: &AddonFolder| {
            !f.dependencies.iter().any(|d| in_group.contains(&d.to_ascii_lowercase()))
                && f.part_of.as_ref().map(|p| !in_group.contains(&p.to_ascii_lowercase())).unwrap_or(true)
        };
        let primary = fs
            .iter()
            .min_by_key(|f| (!is_root(f), f.folder.len(), f.folder.clone()))
            .copied()
            .unwrap();
        let managed = st.installed.get(&key);
        let name = entry
            .map(|e| e.name.clone())
            .unwrap_or_else(|| primary.title.clone());
        let installed_version = managed
            .map(|m| m.version.clone())
            .or_else(|| fs.iter().find_map(|f| f.version.clone()))
            .filter(|v| !v.is_empty() && !v.starts_with('@'));
        let source = pick_source(entry, primary, has_wago, curse, managed)
            .or_else(|| fs.iter().find_map(|f| pick_source(None, f, has_wago, curse, None)));
        let curse_id = entry.and_then(|e| e.curse).or_else(|| fs.iter().find_map(|f| f.curse));
        let supports_forever = fs.iter().any(|f| f.supports_forever());

        // Required dependencies that are not on disk at all.
        let mut missing: Vec<MissingDep> = Vec::new();
        for f in &fs {
            for d in &f.dependencies {
                let dl = d.to_ascii_lowercase();
                if dl.starts_with("blizzard_") || on_disk.contains_key(&dl) || in_group.contains(&dl) {
                    continue;
                }
                if missing.iter().any(|m| m.folder.eq_ignore_ascii_case(d)) {
                    continue;
                }
                let ce = by_folder.get(&dl).copied();
                missing.push(MissingDep {
                    folder: d.clone(),
                    catalog_id: ce.map(|e| e.id.clone()),
                    name: ce.map(|e| e.name.clone()),
                });
            }
        }

        let (status, note) = match &source {
            Some(Source::Wago(_)) if !has_wago => (
                "no-key".to_string(),
                "On Wago: add your free Wago API key in Settings to update".to_string(),
            ),
            Some(_) => ("unknown".to_string(), String::new()),
            None if curse_id.is_some() => (
                "curse-only".to_string(),
                "Only on CurseForge: AddonForge can't update it, but the link works (or try the unofficial CurseForge switch in Settings)".to_string(),
            ),
            None => ("no-source".to_string(), "No update source found in its files".to_string()),
        };
        let mut folder_names: Vec<String> = fs.iter().map(|f| f.folder.clone()).collect();
        folder_names.sort();
        out.push(Package {
            pinned: st.pinned.contains(&key),
            ignored: st.ignored.contains(&key),
            key,
            name,
            folders: folder_names,
            installed_version,
            author: primary.author.clone(),
            notes: entry.map(|e| e.desc.clone()).filter(|d| !d.is_empty()).or_else(|| primary.notes.clone()),
            source_label: source.as_ref().map(|s| s.label()).unwrap_or_default(),
            source_url: source
                .as_ref()
                .map(|s| s.url())
                .or_else(|| curse_id.map(|c| format!("https://www.curseforge.com/projects/{c}")))
                .or_else(|| entry.and_then(|e| e.url.clone())),
            source,
            curse_id,
            catalog_id: entry.map(|e| e.id.clone()),
            notice: entry.and_then(|e| e.notice.clone()).filter(|n| !n.trim().is_empty()),
            supports_forever,
            managed: managed.is_some(),
            missing_deps: missing,
            icon_texture: std::iter::once(primary)
                .chain(fs.iter().copied())
                .find_map(|f| f.icon_texture.clone()),
            remote_version: None,
            remote_forever: None,
            remote_prerelease: false,
            update_available: false,
            status,
            note,
        });
    }
    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    out
}

async fn catalog_cached(app: &App) -> Catalog {
    let mut guard = app.catalog.lock().await;
    if guard.is_none() {
        let loaded = catalog::load(&app.client, false).await;
        logi!("catalogue loaded: {} entries ({})", loaded.0.addons.len(), loaded.1);
        *guard = Some(loaded);
    }
    guard.as_ref().unwrap().0.clone()
}

async fn stats_cached(app: &App) -> Stats {
    catalog_cached(app).await.stats
}

/// Does the catalogue vouch for a Forever build from `src`? Entries from
/// instawow's list are tagged by CurseForge, so they vouch only for it.
fn vouches(e: &CatalogEntry, src: &Source) -> bool {
    e.forever == Some(true) && (e.curse.is_none() || matches!(src, Source::Curse(_)))
}

/// Icon for one thing, via the run cache: `local:<install>|<texture>` or `url:<https>`.
async fn icon_cached(app: &App, cache_key: String, produce: impl std::future::Future<Output = Option<String>>) -> Option<String> {
    if let Some(v) = app.icon_cache.lock().await.get(&cache_key) {
        return v.clone();
    }
    let v = produce.await;
    app.icon_cache.lock().await.insert(cache_key, v.clone());
    v
}

fn entry_hint(cat: &Catalog, pkg: &Package) -> (Option<String>, Option<bool>) {
    let entry = pkg
        .catalog_id
        .as_ref()
        .and_then(|id| cat.addons.iter().find(|e| &e.id == id));
    (entry.and_then(|e| e.asset_hint.clone()), entry.and_then(|e| e.forever))
}

async fn resolve_into(app: &App, pkg: &mut Package, cat: &Catalog) {
    let Some(mut src) = pkg.source.clone() else { return };
    let st = app.state.lock().await.clone();
    if matches!(src, Source::Wago(_)) && !st.has_wago_key() {
        return;
    }
    let (hint, _) = entry_hint(cat, pkg);
    let entry = pkg.catalog_id.as_ref().and_then(|id| cat.addons.iter().find(|e| &e.id == id));
    let opts = ResolveOpts {
        asset_hint: hint.as_deref(),
        wago_key: st.wago_key.as_deref(),
        prerelease: st.allow_prerelease,
    };
    let mut res = sources::resolve(&app.client, &src, opts.clone()).await;
    // No Forever build on GitHub: CurseForge may have one.
    if let (Ok(r), Source::Github(_), Some(c), true) = (&res, &src, pkg.curse_id, st.curse_unofficial) {
        if r.forever == Some(false) {
            src = Source::Curse(c);
            res = sources::resolve(&app.client, &src, opts).await;
            pkg.source_label = src.label();
            pkg.source = Some(src.clone());
        }
    }
    let vouched = entry.map(|e| vouches(e, &src)).unwrap_or(false);
    match res {
        Ok(mut r) => {
            // Some authors list 16001 in the TOC but never added "forever" to
            // release.json. If the catalogue vouches for it, don't scare people.
            if r.forever == Some(false) && vouched {
                r.forever = None;
            }
            let same = pkg
                .installed_version
                .as_deref()
                .map(|v| normalize_version(v) == normalize_version(&r.version))
                .unwrap_or(false);
            pkg.update_available = !same;
            pkg.status = if same { "ok".into() } else { "update".into() };
            pkg.note = match r.forever {
                Some(false) => "Latest release does not list a Forever build".into(),
                _ => String::new(),
            };
            pkg.remote_forever = r.forever;
            pkg.remote_prerelease = r.prerelease;
            pkg.remote_version = Some(r.version);
        }
        Err(e) => {
            loge!("check {}: {e}", pkg.key);
            pkg.status = "error".into();
            pkg.note = e.to_string();
        }
    }
}

async fn check_all(app: Shared, pkgs: &mut [Package], cat: &Catalog) {
    let sem = Arc::new(Semaphore::new(6));
    let mut set = tokio::task::JoinSet::new();
    for (i, p) in pkgs.iter().enumerate() {
        if p.source.is_none() || p.ignored {
            continue;
        }
        let mut p = p.clone();
        let cat = cat.clone();
        let sem = sem.clone();
        let app_ref = app.clone();
        set.spawn(async move {
            let _permit = sem.acquire().await;
            resolve_into(&app_ref, &mut p, &cat).await;
            (i, p)
        });
    }
    while let Some(Ok((i, p))) = set.join_next().await {
        pkgs[i] = p;
    }
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
struct Settings {
    version: String,
    build: u32,
    install_path: Option<String>,
    install: Option<WowInstall>,
    has_wago_key: bool,
    allow_prerelease: bool,
    curse_unofficial: bool,
    ignored_count: usize,
    state_file: String,
    log_file: String,
    catalog_source: Option<String>,
    catalog_updated: Option<String>,
    catalog_count: usize,
    wago_key_url: String,
    repo_url: String,
    /// `ADDONFORGE_TAB=browse|settings` opens on that tab (screenshots, testing).
    start_tab: Option<String>,
    /// Android release name (`2026.10.14`); None for local builds.
    release: Option<String>,
}

#[tauri::command]
async fn get_settings(app: State<'_, Shared>) -> Result<Settings, String> {
    let st = app.state.lock().await.clone();
    let install = st.install_path.as_deref().and_then(|p| wow::describe_install(Path::new(p)));
    let cat = app.catalog.lock().await.clone();
    Ok(Settings {
        version: selfupdate::version().into(),
        build: selfupdate::build(),
        install_path: st.install_path.clone(),
        install,
        has_wago_key: st.has_wago_key(),
        allow_prerelease: st.allow_prerelease,
        curse_unofficial: st.curse_unofficial,
        ignored_count: st.ignored.len(),
        state_file: AppState::path().to_string_lossy().to_string(),
        log_file: log::path().to_string_lossy().to_string(),
        catalog_source: cat.as_ref().map(|c| c.1.to_string()),
        catalog_updated: cat.as_ref().map(|c| c.0.updated.clone()),
        catalog_count: cat.as_ref().map(|c| c.0.addons.len()).unwrap_or(0),
        wago_key_url: sources::wago::KEY_URL.into(),
        repo_url: REPO_URL.into(),
        start_tab: std::env::var("ADDONFORGE_TAB").ok().filter(|t| !t.is_empty()),
        release: selfupdate::release().map(Into::into),
    })
}

#[tauri::command]
async fn find_installs(app: State<'_, Shared>) -> Result<Vec<WowInstall>, String> {
    let found = tokio::task::spawn_blocking(wow::find_installs).await.map_err(err)?;
    let mut st = app.state.lock().await;
    if st.install_path.is_none() {
        if let Some(f) = found.iter().find(|i| i.flavor == "forever") {
            st.install_path = Some(f.path.clone());
            st.save().map_err(err)?;
            logi!("auto-selected install {}", f.path);
        }
    }
    Ok(found)
}

#[tauri::command]
async fn set_install_path(app: State<'_, Shared>, path: String) -> Result<WowInstall, String> {
    let p = PathBuf::from(path.trim());
    let info = wow::describe_install(&p)
        .ok_or_else(|| "That folder doesn't look like a WoW product folder (expected e.g. .../_classic_beta_ with Interface/AddOns inside)".to_string())?;
    let mut st = app.state.lock().await;
    st.install_path = Some(info.path.clone());
    st.save().map_err(err)?;
    logi!("install set to {}", info.path);
    Ok(info)
}

#[tauri::command]
async fn set_wago_key(app: State<'_, Shared>, key: String) -> Result<bool, String> {
    let mut st = app.state.lock().await;
    let k = key.trim().to_string();
    st.wago_key = if k.is_empty() { None } else { Some(k) };
    st.save().map_err(err)?;
    Ok(st.wago_key.is_some())
}

#[tauri::command]
async fn set_prefs(app: State<'_, Shared>, allow_prerelease: Option<bool>, curse_unofficial: Option<bool>) -> Result<(), String> {
    let mut st = app.state.lock().await;
    if let Some(v) = allow_prerelease {
        st.allow_prerelease = v;
        logi!("allow_prerelease = {v}");
    }
    if let Some(v) = curse_unofficial {
        st.curse_unofficial = v;
        logi!("curse_unofficial = {v}");
    }
    st.save().map_err(err)
}

#[tauri::command]
async fn set_pin(app: State<'_, Shared>, key: String, pinned: bool) -> Result<(), String> {
    let mut st = app.state.lock().await;
    if pinned {
        st.pinned.insert(key);
    } else {
        st.pinned.remove(&key);
    }
    st.save().map_err(err)
}

#[tauri::command]
async fn set_ignore(app: State<'_, Shared>, key: String, ignored: bool) -> Result<(), String> {
    let mut st = app.state.lock().await;
    if ignored {
        st.ignored.insert(key);
    } else {
        st.ignored.remove(&key);
    }
    st.save().map_err(err)
}

async fn current_install(app: &App) -> Result<String, String> {
    let st = app.state.lock().await;
    st.install_path
        .clone()
        .ok_or_else(|| "No WoW install selected yet".to_string())
}

async fn scan_packages(app: &App) -> Result<Vec<Package>, String> {
    let install = current_install(app).await?;
    let cat = catalog_cached(app).await;
    let folders = tokio::task::spawn_blocking(move || wow::scan_addons(Path::new(&install)))
        .await
        .map_err(err)?;
    let st = app.state.lock().await.clone();
    Ok(build_packages(&st, &cat, &folders))
}

#[tauri::command]
async fn scan(app: State<'_, Shared>) -> Result<Vec<Package>, String> {
    scan_packages(&app).await
}

#[tauri::command]
async fn check_updates(app: State<'_, Shared>) -> Result<Vec<Package>, String> {
    let mut pkgs = scan_packages(&app).await?;
    let cat = catalog_cached(&app).await;
    check_all(app.inner().clone(), &mut pkgs, &cat).await;
    let n = pkgs.iter().filter(|p| p.status == "update").count();
    logi!("check: {} packages, {} updates", pkgs.len(), n);
    Ok(pkgs)
}

async fn install_from(
    app: &App,
    key: &str,
    src: &Source,
    asset_hint: Option<&str>,
    allow_non_forever: bool,
) -> Result<Installed, String> {
    let install = current_install(app).await?;
    let addons = addons_dir(&install);
    let st = app.state.lock().await.clone();
    if matches!(src, Source::Curse(_)) && !st.curse_unofficial {
        return Err("This addon is only on CurseForge: turn on the unofficial CurseForge switch in Settings first".into());
    }
    let opts = ResolveOpts {
        asset_hint,
        wago_key: st.wago_key.as_deref(),
        prerelease: st.allow_prerelease,
    };
    let remote = sources::resolve(&app.client, src, opts).await.map_err(err)?;
    let vouched = match key.strip_prefix("cat:") {
        Some(id) => catalog_cached(app).await.addons.iter().find(|e| e.id == id).map(|e| vouches(e, src)).unwrap_or(false),
        None => false,
    };
    if remote.forever == Some(false) && !allow_non_forever && !vouched {
        return Err("NOT_FOREVER".into());
    }
    logi!("install {key} from {} {} -> {}", src.label(), remote.version, remote.download_url);
    let tmp = addons.join(install::TMP_DIR).join("dl");
    let zip = install::download(&app.client, &remote.download_url, &tmp, &remote.filename)
        .await
        .map_err(|e| {
            loge!("download {key}: {e}");
            e.to_string()
        })?;
    let addons2 = addons.clone();
    let folders = tokio::task::spawn_blocking(move || install::place(&zip, &addons2))
        .await
        .map_err(err)?
        .map_err(|e| {
            loge!("place {key}: {e}");
            e.to_string()
        })?;
    let mut rec = Installed {
        source: src.kind().into(),
        source_id: src.id(),
        version: remote.version.clone(),
        folders,
        installed_at: state::now_secs(),
    };
    // From CurseForge, but its TOC names a GitHub repo with a Forever build:
    // future updates come from GitHub, the official way.
    if matches!(src, Source::Curse(_)) {
        if let Some(repo) = github_from_toc(&install, &rec.folders).await {
            let opts = ResolveOpts { asset_hint: None, wago_key: None, prerelease: st.allow_prerelease };
            match sources::resolve(&app.client, &Source::Github(repo.clone()), opts).await {
                Ok(r) if r.forever != Some(false) => {
                    logi!("{key}: updates from GitHub {repo} from now on");
                    rec.source = "github".into();
                    rec.source_id = repo;
                }
                _ => {}
            }
        }
    }
    let mut st = app.state.lock().await;
    st.installed.insert(key.to_string(), rec.clone());
    st.save().map_err(err)?;
    logi!("installed {key} {} ({} folders)", rec.version, rec.folders.len());
    Ok(rec)
}

/// A GitHub repo named by the TOCs (X-Website) of these installed folders.
async fn github_from_toc(install: &str, folders: &[String]) -> Option<String> {
    let install = install.to_string();
    let names: Vec<String> = folders.iter().map(|f| f.to_ascii_lowercase()).collect();
    let scanned = tokio::task::spawn_blocking(move || wow::scan_addons(Path::new(&install))).await.ok()?;
    scanned
        .iter()
        .filter(|f| names.contains(&f.folder.to_ascii_lowercase()))
        .find_map(|f| f.website.as_deref().and_then(sources::parse_github_repo))
}

/// Install, and when GitHub has no Forever build fall back to CurseForge
/// (unofficial switch on).
async fn install_or_curse(
    app: &App,
    key: &str,
    src: &Source,
    curse_id: Option<u64>,
    asset_hint: Option<&str>,
    allow_non_forever: bool,
) -> Result<Installed, String> {
    match install_from(app, key, src, asset_hint, allow_non_forever).await {
        Err(e) if e == "NOT_FOREVER" && matches!(src, Source::Github(_)) => {
            let curse = app.state.lock().await.curse_unofficial;
            match curse_id.filter(|_| curse) {
                Some(c) => {
                    logi!("{key}: no Forever build on GitHub, trying CurseForge");
                    install_from(app, key, &Source::Curse(c), None, allow_non_forever).await
                }
                None => Err(e),
            }
        }
        r => r,
    }
}

async fn refreshed(app: &App, key: &str) -> Result<Package, String> {
    let cat = catalog_cached(app).await;
    let mut pkgs = scan_packages(app).await?;
    let mut p = pkgs
        .drain(..)
        .find(|p| p.key == key)
        .ok_or_else(|| "Installed, but its folders didn't show up on rescan".to_string())?;
    resolve_into(app, &mut p, &cat).await;
    Ok(p)
}

#[tauri::command]
async fn update_package(
    app: State<'_, Shared>,
    key: String,
    allow_non_forever: Option<bool>,
) -> Result<Package, String> {
    let pkgs = scan_packages(&app).await?;
    let pkg = pkgs
        .into_iter()
        .find(|p| p.key == key)
        .ok_or_else(|| "Addon not found on disk any more".to_string())?;
    let src = pkg.source.clone().ok_or_else(|| "This addon has no update source".to_string())?;
    let cat = catalog_cached(&app).await;
    let (hint, _) = entry_hint(&cat, &pkg);
    install_or_curse(&app, &key, &src, pkg.curse_id, hint.as_deref(), allow_non_forever.unwrap_or(false)).await?;
    refreshed(&app, &key).await
}

#[derive(Serialize, Clone)]
struct CatalogRow {
    #[serde(flatten)]
    entry: CatalogEntry,
    installed: bool,
    key: String,
    downloads: Option<u64>,
    updated_at: Option<String>,
    stat_source: Option<String>,
}

#[tauri::command]
async fn catalog_list(app: State<'_, Shared>, refresh: Option<bool>) -> Result<Vec<CatalogRow>, String> {
    if refresh.unwrap_or(false) {
        let loaded = catalog::load(&app.client, true).await;
        logi!("catalogue refreshed: {} entries ({})", loaded.0.addons.len(), loaded.1);
        *app.catalog.lock().await = Some(loaded);
    }
    let cat = catalog_cached(&app).await;
    let stats = stats_cached(&app).await;
    let installed_keys: Vec<String> = match scan_packages(&app).await {
        Ok(p) => p.into_iter().map(|p| p.key).collect(),
        Err(_) => Vec::new(),
    };
    Ok(cat
        .addons
        .into_iter()
        .map(|e| {
            let key = format!("cat:{}", e.id);
            let st = stats.entries.get(&e.id);
            CatalogRow {
                installed: installed_keys.contains(&key),
                key,
                downloads: st.and_then(|s| s.downloads),
                updated_at: st.and_then(|s| s.updated_at.clone()),
                stat_source: st.map(|s| s.source.clone()).filter(|s| !s.is_empty()),
                entry: e,
            }
        })
        .collect())
}

#[derive(Serialize, Clone)]
struct BundleAddon {
    id: String,
    name: String,
    desc: String,
    installable: bool,
    needs_key: bool,
    installed: bool,
    forever: Option<bool>,
}

#[derive(Serialize, Clone)]
struct BundleView {
    id: String,
    name: String,
    desc: String,
    note: String,
    addons: Vec<BundleAddon>,
}

#[tauri::command]
async fn bundles(app: State<'_, Shared>) -> Result<Vec<BundleView>, String> {
    let cat = catalog_cached(&app).await;
    let (has_wago, curse) = {
        let st = app.state.lock().await;
        (st.has_wago_key(), st.curse_unofficial)
    };
    let installed_keys: Vec<String> = match scan_packages(&app).await {
        Ok(p) => p.into_iter().map(|p| p.key).collect(),
        Err(_) => Vec::new(),
    };
    Ok(cat
        .bundles
        .iter()
        .map(|b| BundleView {
            id: b.id.clone(),
            name: b.name.clone(),
            desc: b.desc.clone(),
            note: b.note.clone(),
            addons: b
                .addons
                .iter()
                .filter_map(|id| cat.addons.iter().find(|e| &e.id == id))
                .map(|e| {
                    let direct = e.github.is_some() || e.wowi.is_some() || e.tukui.is_some() || (curse && e.curse.is_some());
                    BundleAddon {
                        id: e.id.clone(),
                        name: e.name.clone(),
                        desc: e.desc.clone(),
                        installable: direct || (e.wago.is_some() && has_wago),
                        needs_key: !direct && e.wago.is_some() && !has_wago,
                        installed: installed_keys.contains(&format!("cat:{}", e.id)),
                        forever: e.forever,
                    }
                })
                .collect(),
        })
        .collect())
}

/// Icons for installed packages: the addon's own icon file, else none.
#[tauri::command]
async fn package_icons(app: State<'_, Shared>, keys: Vec<String>) -> Result<HashMap<String, String>, String> {
    let install = current_install(&app).await?;
    let pkgs = scan_packages(&app).await?;
    let mut out = HashMap::new();
    for p in pkgs.into_iter().filter(|p| keys.contains(&p.key)) {
        let mut icon = None;
        if let Some(tex) = &p.icon_texture {
            let (inst, tex2) = (install.clone(), tex.clone());
            icon = icon_cached(&app, format!("local:{inst}|{tex2}"), async move {
                tokio::task::spawn_blocking(move || icons::local_icon(Path::new(&inst), &tex2)).await.ok().flatten()
            })
            .await;
        }
        if let Some(i) = icon {
            out.insert(p.key, i);
        }
    }
    Ok(out)
}

/// Longer description + preview image for the details window: the README
/// excerpt and first screenshot for GitHub, cfwidget's summary and
/// thumbnail for CurseForge, nothing for other sources.
#[tauri::command]
async fn addon_details(app: State<'_, Shared>, id: String) -> Result<icons::Details, String> {
    let cat = catalog_cached(&app).await;
    let Some(e) = cat.addons.iter().find(|e| e.id == id) else { return Ok(icons::Details::default()) };
    match (&e.github, e.curse) {
        (Some(repo), _) => Ok(icons::github_details(&app.client, repo).await),
        (None, Some(id)) => Ok(icons::curse_details(&app.client, id).await),
        _ => Ok(icons::Details::default()),
    }
}

#[tauri::command]
async fn install_catalog(
    app: State<'_, Shared>,
    id: String,
    allow_non_forever: Option<bool>,
) -> Result<Package, String> {
    let cat = catalog_cached(&app).await;
    let e = cat
        .addons
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| "Not in the catalogue".to_string())?;
    let st = app.state.lock().await.clone();
    let has_wago = st.has_wago_key();
    let src = pick_source(Some(&e), &AddonFolder::default(), has_wago, st.curse_unofficial, None)
        .ok_or_else(|| "This catalogue entry has no downloadable source".to_string())?;
    if matches!(src, Source::Wago(_)) && !has_wago {
        return Err("This addon is on Wago: add your free Wago API key in Settings first".into());
    }
    let key = format!("cat:{}", e.id);
    install_or_curse(&app, &key, &src, e.curse, e.asset_hint.as_deref(), allow_non_forever.unwrap_or(false)).await?;
    refreshed(&app, &key).await
}

/// Install straight from a source the user typed or imported.
#[tauri::command]
async fn install_source(
    app: State<'_, Shared>,
    source: Source,
    key: Option<String>,
    allow_non_forever: Option<bool>,
) -> Result<Package, String> {
    let key = key.unwrap_or_else(|| format!("{}:{}", source.kind(), source.id()));
    install_from(&app, &key, &source, None, allow_non_forever.unwrap_or(false)).await?;
    refreshed(&app, &key).await
}

#[tauri::command]
async fn install_github(
    app: State<'_, Shared>,
    repo: String,
    allow_non_forever: Option<bool>,
) -> Result<Package, String> {
    let repo = sources::parse_github_repo(&repo)
        .ok_or_else(|| "Enter a GitHub repo as owner/name or paste its URL".to_string())?;
    // If the catalogue already knows this repo, use its key so it groups properly.
    let cat = catalog_cached(&app).await;
    let key = cat
        .addons
        .iter()
        .find(|e| e.github.as_deref().map(|g| g.eq_ignore_ascii_case(&repo)).unwrap_or(false))
        .map(|e| format!("cat:{}", e.id))
        .unwrap_or_else(|| format!("github:{repo}"));
    let src = Source::Github(repo);
    install_from(&app, &key, &src, None, allow_non_forever.unwrap_or(false)).await?;
    refreshed(&app, &key).await
}

#[tauri::command]
async fn remove_package(app: State<'_, Shared>, key: String) -> Result<Vec<String>, String> {
    let install = current_install(&app).await?;
    let pkgs = scan_packages(&app).await?;
    let pkg = pkgs
        .into_iter()
        .find(|p| p.key == key)
        .ok_or_else(|| "Addon not found".to_string())?;
    let addons = addons_dir(&install);
    let gone = tokio::task::spawn_blocking(move || install::remove_folders(&addons, &pkg.folders))
        .await
        .map_err(err)?
        .map_err(err)?;
    let mut st = app.state.lock().await;
    st.installed.remove(&key);
    st.pinned.remove(&key);
    st.ignored.remove(&key);
    st.save().map_err(err)?;
    logi!("removed {key}: {:?}", gone);
    Ok(gone)
}

// ---------------------------------------------------------------- import / export

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ExportItem {
    name: String,
    key: String,
    #[serde(default)]
    source: Option<Source>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    folders: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ExportFile {
    addonforge: u32,
    flavor: String,
    exported: u64,
    addons: Vec<ExportItem>,
}

async fn export_data(app: &App) -> Result<ExportFile, String> {
    let pkgs = scan_packages(app).await?;
    Ok(ExportFile {
        addonforge: 1,
        flavor: "forever".into(),
        exported: state::now_secs(),
        addons: pkgs
            .into_iter()
            .filter(|p| !p.ignored)
            .map(|p| ExportItem {
                name: p.name,
                key: p.key,
                source: p.source,
                version: p.installed_version,
                folders: p.folders,
            })
            .collect(),
    })
}

#[tauri::command]
async fn export_list(app: State<'_, Shared>, path: String) -> Result<usize, String> {
    let data = export_data(&app).await?;
    let n = data.addons.len();
    std::fs::write(&path, serde_json::to_vec_pretty(&data).map_err(err)?).map_err(err)?;
    logi!("exported {n} addons to {path}");
    Ok(n)
}

#[derive(Serialize, Clone)]
struct ImportRow {
    name: String,
    key: String,
    source: Option<Source>,
    installed: bool,
    installable: bool,
    why: String,
}

#[tauri::command]
async fn import_list(app: State<'_, Shared>, path: String) -> Result<Vec<ImportRow>, String> {
    let bytes = std::fs::read(&path).map_err(err)?;
    let file: ExportFile = serde_json::from_slice(&bytes).map_err(|e| format!("not an AddonForge list: {e}"))?;
    let have: Vec<String> = scan_packages(&app).await?.into_iter().map(|p| p.key).collect();
    let (has_wago, curse) = {
        let st = app.state.lock().await;
        (st.has_wago_key(), st.curse_unofficial)
    };
    Ok(file
        .addons
        .into_iter()
        .map(|it| {
            let installed = have.contains(&it.key);
            let (installable, why) = match &it.source {
                None => (false, "no source (probably CurseForge-only or a personal addon)".into()),
                Some(Source::Wago(_)) if !has_wago => (false, "needs a Wago key".into()),
                Some(Source::Curse(_)) if !curse => (false, "CurseForge only (turn on the unofficial switch in Settings)".into()),
                Some(_) => (true, String::new()),
            };
            ImportRow { name: it.name, key: it.key, source: it.source, installed, installable, why }
        })
        .collect())
}

// ---------------------------------------------------------------- self-update

#[tauri::command]
async fn check_self_update(app: State<'_, Shared>) -> Result<selfupdate::SelfUpdate, String> {
    let url = std::env::var("ADDONFORGE_UPDATE_URL").ok();
    let mut r = selfupdate::check(&app.client, url.as_deref()).await;
    let st = app.state.lock().await;
    if st.skip_self_update.as_deref() == Some(r.tag.as_str()) {
        r.available = false;
    }
    if let Some(e) = &r.error {
        logi!("self-update check: {e}");
    } else {
        logi!("self-update check: latest {} (available: {})", r.tag, r.available);
    }
    Ok(r)
}

#[tauri::command]
async fn skip_self_update(app: State<'_, Shared>, tag: String) -> Result<(), String> {
    let mut st = app.state.lock().await;
    st.skip_self_update = Some(tag);
    st.save().map_err(err)
}

/// First launch of a newer build: return that build's release notes once.
/// Never on a fresh install (nothing to compare with), never for dev builds.
#[derive(Serialize)]
struct WhatsNew {
    version: String,
    build: u32,
    notes: String,
}

#[tauri::command]
async fn whats_new(app: State<'_, Shared>) -> Result<Option<WhatsNew>, String> {
    let current = selfupdate::build();
    if current == 0 {
        return Ok(None);
    }
    let (last, has_history) = {
        let st = app.state.lock().await;
        (st.last_seen_build, !st.installed.is_empty())
    };
    if last == current {
        return Ok(None);
    }
    // Existing user if: we recorded a previous build, or they already have
    // addons managed by AddonForge (people on builds before this field
    // existed). Otherwise fresh install.
    let existing = last != 0 || has_history;
    let mut result = None;
    if existing {
        let url = std::env::var("ADDONFORGE_UPDATE_URL").ok();
        let r = selfupdate::check(&app.client, url.as_deref()).await;
        if let Some(l) = r.latest {
            if l.build == current && !l.notes.trim().is_empty() {
                result = Some(WhatsNew { version: l.version, build: l.build, notes: l.notes });
            }
        }
    }
    let mut st = app.state.lock().await;
    st.last_seen_build = current;
    st.save().map_err(err)?;
    Ok(result)
}

// ---------------------------------------------------------------- diagnostics

#[tauri::command]
async fn diagnostics(app: State<'_, Shared>) -> Result<String, String> {
    let st = app.state.lock().await.clone();
    let install = st.install_path.as_deref().and_then(|p| wow::describe_install(Path::new(p)));
    let pkgs = scan_packages(&app).await.unwrap_or_default();
    let mut s = String::new();
    s.push_str(&format!("AddonForge v{} build {}\n", selfupdate::version(), selfupdate::build()));
    s.push_str(&format!("OS: {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    s.push_str(&format!("Exe: {}\n", std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()));
    match &install {
        Some(i) => s.push_str(&format!("Install: {} ({}, {})\n", i.path, i.product, i.label)),
        None => s.push_str("Install: none selected\n"),
    }
    s.push_str(&format!(
        "Wago key: {} | pre-releases: {} | CurseForge (unofficial): {} | pinned: {} | ignored: {}\n",
        if st.has_wago_key() { "yes" } else { "no" },
        st.allow_prerelease,
        st.curse_unofficial,
        st.pinned.len(),
        st.ignored.len()
    ));
    s.push_str(&format!("Addons on disk: {}\n", pkgs.len()));
    for p in &pkgs {
        s.push_str(&format!(
            "  - {} [{}] {} {}{}\n",
            p.name,
            p.status,
            p.installed_version.clone().unwrap_or_default(),
            p.source_label,
            if p.managed { " (managed)" } else { "" }
        ));
    }
    s.push_str("\n--- log tail ---\n");
    s.push_str(&log::tail(40));
    Ok(s)
}

// ---------------------------------------------------------------- boot

fn make_app() -> Shared {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("http client");
    let st = AppState::load();
    if let Some(p) = &st.install_path {
        install::clean_tmp(&addons_dir(p));
    }
    Arc::new(App {
        client,
        state: Mutex::new(st),
        catalog: Mutex::new(None),
        icon_cache: Mutex::new(HashMap::new()),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logi!("AddonForge v{} build {} starting", selfupdate::version(), selfupdate::build());
    let app = make_app();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            get_settings,
            find_installs,
            set_install_path,
            set_wago_key,
            set_prefs,
            set_pin,
            set_ignore,
            scan,
            check_updates,
            update_package,
            catalog_list,
            bundles,
            package_icons,
            addon_details,
            install_catalog,
            install_source,
            install_github,
            remove_package,
            export_list,
            import_list,
            check_self_update,
            skip_self_update,
            whats_new,
            diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AddonForge");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, deps: &[&str], part_of: Option<&str>) -> AddonFolder {
        AddonFolder {
            folder: name.into(),
            title: name.into(),
            version: Some("1.0".into()),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            part_of: part_of.map(|s| s.to_string()),
            interfaces: vec![16001],
            forever_interface: true,
            ..Default::default()
        }
    }

    #[test]
    fn modules_group_under_their_core_without_catalogue() {
        let st = AppState::default();
        let cat = Catalog::default();
        let folders = vec![
            folder("DBM-Core", &[], None),
            folder("DBM-GUI", &["DBM-Core"], None),
            folder("DBM-StatusBarTimers", &["DBM-Core"], None),
            folder("BigWigs", &[], None),
            folder("BigWigs_Core", &["BigWigs"], None),
            folder("BigWigs_Sporefall", &["BigWigs_Core"], None),
            folder("Plater", &[], None),
            folder("LibStub", &[], None),
            folder("SomeAddon", &["LibStub"], None),
            folder("ModuleX", &[], Some("Plater")),
        ];
        let pkgs = build_packages(&st, &cat, &folders);
        let names: Vec<(String, usize)> = pkgs.iter().map(|p| (p.name.clone(), p.folders.len())).collect();
        assert!(names.contains(&("DBM-Core".into(), 3)), "{names:?}");
        assert!(names.contains(&("BigWigs".into(), 3)), "{names:?}");
        assert!(names.contains(&("Plater".into(), 2)), "{names:?}");
        assert!(names.contains(&("LibStub".into(), 1)), "{names:?}");
        assert!(names.contains(&("SomeAddon".into(), 1)), "{names:?}");
        assert_eq!(pkgs.len(), 5);
    }

    #[test]
    fn managed_folders_stay_with_their_package() {
        let mut st = AppState::default();
        st.installed.insert(
            "cat:bigwigs".into(),
            Installed { source: "github".into(), source_id: "x".into(), version: "v1".into(), folders: vec!["BigWigs".into(), "BigWigs_Zone".into()], installed_at: 0 },
        );
        let cat: Catalog = serde_json::from_str(r#"{"addons":[{"id":"bigwigs","name":"BigWigs","github":"a/b","folders":["BigWigs"]}]}"#).unwrap();
        let folders = vec![folder("BigWigs", &[], None), folder("BigWigs_Zone", &[], None)];
        let pkgs = build_packages(&st, &cat, &folders);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].folders.len(), 2);
        assert_eq!(pkgs[0].installed_version.as_deref(), Some("v1"));
        assert!(pkgs[0].managed);
    }

    #[test]
    fn missing_dependencies_are_reported_and_matched_to_catalogue() {
        let st = AppState::default();
        let cat: Catalog = serde_json::from_str(r#"{"addons":[{"id":"titan","name":"Titan Panel","github":"a/b","folders":["Titan"]}]}"#).unwrap();
        let folders = vec![
            folder("TitanReputation", &["Titan", "Blizzard_Foo", "!BugGrabber"], None),
            folder("BugSack", &["!BugGrabber"], None),
        ];
        let pkgs = build_packages(&st, &cat, &folders);
        let tr = pkgs.iter().find(|p| p.name == "TitanReputation").unwrap();
        let deps: Vec<&str> = tr.missing_deps.iter().map(|d| d.folder.as_str()).collect();
        assert_eq!(deps, vec!["Titan", "!BugGrabber"]);
        assert_eq!(tr.missing_deps[0].catalog_id.as_deref(), Some("titan"));
        assert!(tr.missing_deps[1].catalog_id.is_none());
    }

    #[test]
    fn user_agent_names_the_fork() {
        assert!(USER_AGENT.starts_with("AddonForge-android/"), "{USER_AGENT}");
        assert!(USER_AGENT.contains("github.com/grutamu/AddonForge-android"), "{USER_AGENT}");
        assert!(!USER_AGENT.contains("Topher121"), "{USER_AGENT}");
    }

    #[test]
    fn managed_source_wins_and_github_key_is_used() {
        let mut st = AppState::default();
        st.installed.insert(
            "github:someone/Thing".into(),
            Installed { source: "github".into(), source_id: "someone/Thing".into(), version: "2.0".into(), folders: vec!["Thing".into()], installed_at: 0 },
        );
        let pkgs = build_packages(&st, &Catalog::default(), &[folder("Thing", &[], None)]);
        assert_eq!(pkgs[0].key, "github:someone/Thing");
        assert_eq!(pkgs[0].source, Some(Source::Github("someone/Thing".into())));
    }

    #[test]
    fn curseforge_only_with_the_switch_and_only_as_last_resort() {
        let mut st = AppState::default();
        let cat: Catalog = serde_json::from_str(
            r#"{"addons":[{"id":"details","name":"Details","curse":61284,"folders":["Details"]},{"id":"both","name":"Both","github":"a/b","curse":1,"folders":["Both"]},{"id":"wago","name":"W","wago":"xyz","curse":2,"folders":["W"]}]}"#,
        )
        .unwrap();
        let folders = vec![folder("Details", &[], None), folder("Both", &[], None), folder("W", &[], None)];
        let by_name = |pkgs: &[Package], n: &str| pkgs.iter().find(|p| p.name == n).unwrap().clone();

        let off = build_packages(&st, &cat, &folders);
        assert_eq!(by_name(&off, "Details").status, "curse-only");
        assert_eq!(by_name(&off, "W").status, "no-key");

        st.curse_unofficial = true;
        let on = build_packages(&st, &cat, &folders);
        assert_eq!(by_name(&on, "Details").source, Some(Source::Curse(61284)));
        assert_eq!(by_name(&on, "Both").source, Some(Source::Github("a/b".into())));
        assert_eq!(by_name(&on, "W").source, Some(Source::Curse(2)));

        // Installed from CurseForge, then the switch goes off: back to the link.
        st.installed.insert(
            "cat:details".into(),
            Installed { source: "curse".into(), source_id: "61284".into(), version: "1".into(), folders: vec!["Details".into()], installed_at: 0 },
        );
        st.curse_unofficial = false;
        assert_eq!(by_name(&build_packages(&st, &cat, &folders), "Details").status, "curse-only");
    }

    #[test]
    fn folder_matching_prefers_the_addon_named_after_the_folder() {
        let st = AppState::default();
        let mut bundle: Vec<String> = (0..40).map(|i| format!("X{i}")).collect();
        bundle.push("!BugGrabber".into());
        let cat = Catalog {
            addons: vec![
                CatalogEntry { id: "bundle".into(), name: "Big Bundle".into(), curse: Some(1), folders: bundle, ..Default::default() },
                CatalogEntry { id: "sack".into(), name: "BugSack".into(), curse: Some(2), folders: vec!["BugSack".into(), "!BugGrabber".into()], ..Default::default() },
                CatalogEntry { id: "grabber".into(), name: "BugGrabber".into(), curse: Some(3), folders: vec!["!BugGrabber".into(), "Extra".into(), "More".into()], ..Default::default() },
                CatalogEntry { id: "repack".into(), name: "Musca Auras".into(), curse: Some(4), folders: vec!["WeakAuras".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let mut wa = folder("WeakAuras", &[], None);
        wa.curse = Some(65387);
        let pkgs = build_packages(&st, &cat, &[folder("!BugGrabber", &[], None), wa, folder("Needs", &["BugSack"], None)]);
        let key = |n: &str| pkgs.iter().find(|p| p.folders.iter().any(|f| f == n)).unwrap().key.clone();
        assert_eq!(key("!BugGrabber"), "cat:grabber", "named after the folder beats smaller");
        assert_eq!(key("WeakAuras"), "curse:65387", "its own CurseForge id says it isn't the repack");
        let needs = pkgs.iter().find(|p| p.name == "Needs").unwrap();
        assert_eq!(needs.missing_deps[0].catalog_id.as_deref(), Some("sack"));
    }
}
