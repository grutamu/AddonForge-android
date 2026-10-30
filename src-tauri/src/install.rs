use crate::state::Installed;
use anyhow::{anyhow, Context};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Inside AddOns so the final move is a same-volume rename.
pub const TMP_DIR: &str = ".addonforge-tmp";
pub const BACKUP_DIR: &str = ".addonforge-backup";
const MANIFEST: &str = "backup.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Backup {
    pub key: String,
    pub version: Option<String>,
    /// None when AddonForge hadn't installed it.
    pub record: Option<Installed>,
    pub folders: Vec<String>,
    /// Folders of the version on disk, which a restore swaps out.
    pub current: Vec<String>,
    pub saved_at: u64,
}

pub struct Keep {
    pub key: String,
    pub version: Option<String>,
    pub record: Option<Installed>,
}

pub struct Placed {
    pub folders: Vec<String>,
    pub kept: bool,
}

const FORBIDDEN_EXT: &[&str] = &[
    "exe", "dll", "sys", "scr", "com", "pif", "cpl", "msi", "msp", "bat", "cmd", "ps1", "psm1",
    "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "jar", "py", "sh", "lnk", "reg", "inf",
    "ocx", "drv", "app", "dmg", "apk",
];

fn unique() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{n:x}")
}

pub fn clean_tmp(addons_dir: &Path) {
    let _ = fs::remove_dir_all(addons_dir.join(TMP_DIR));
}

pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest_dir: &Path,
    filename: &str,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(dest_dir)?;
    let safe: String = filename
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    let safe = if safe.to_ascii_lowercase().ends_with(".zip") { safe } else { format!("{safe}.zip") };
    let path = dest_dir.join(format!("{}-{}", unique(), safe));
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("download failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("download refused: {url}"))?;
    let mut file = fs::File::create(&path)?;
    let mut stream = resp.bytes_stream();
    let mut total: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total += chunk.len() as u64;
        if total > 512 * 1024 * 1024 {
            anyhow::bail!("download exceeded 512 MB; refusing");
        }
        file.write_all(&chunk)?;
    }
    file.flush()?;
    Ok(path)
}

fn has_toc(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".toc"))
        })
        .unwrap_or(false)
}

fn addon_dirs(extracted: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(extracted)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    if dirs.is_empty() {
        anyhow::bail!("zip contains no addon folder");
    }
    // Unwrap a single wrapper directory like "MyAddon-1.2/".
    if dirs.len() == 1 && !has_toc(&dirs[0]) {
        let inner: Vec<PathBuf> = fs::read_dir(&dirs[0])?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && has_toc(p))
            .collect();
        if !inner.is_empty() {
            return Ok(inner);
        }
    }
    let with_toc: Vec<PathBuf> = dirs.iter().filter(|d| has_toc(d)).cloned().collect();
    if with_toc.is_empty() {
        anyhow::bail!("zip has folders but none contains a .toc file");
    }
    Ok(with_toc)
}

/// Errors on anything executable; returns the file count.
pub fn safety_scan(root: &Path) -> anyhow::Result<usize> {
    fn walk(root: &Path, dir: &Path, depth: usize, count: &mut usize) -> anyhow::Result<()> {
        if depth > 32 {
            anyhow::bail!("zip nests folders absurdly deep");
        }
        for e in fs::read_dir(dir)?.flatten() {
            let p = e.path();
            let ft = e.file_type()?;
            if ft.is_symlink() {
                anyhow::bail!("zip contains a symbolic link: {}", p.display());
            }
            if ft.is_dir() {
                walk(root, &p, depth + 1, count)?;
                continue;
            }
            *count += 1;
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            let ext = lower.rsplit('.').next().unwrap_or("");
            if lower.contains('.') && FORBIDDEN_EXT.contains(&ext) {
                anyhow::bail!(
                    "SAFETY: refused, the zip contains an executable file ({}) which no WoW addon needs",
                    p.strip_prefix(root).unwrap_or(&p).display()
                );
            }
            // Catch disguised executables by their magic bytes.
            if !matches!(ext, "lua" | "xml" | "toc" | "tga" | "blp" | "png" | "jpg" | "jpeg" | "gif" | "ttf" | "otf" | "mp3" | "ogg" | "wav" | "txt" | "md" | "json" | "html" | "css" | "csv") {
                if let Ok(mut f) = fs::File::open(&p) {
                    let mut head = [0u8; 4];
                    use std::io::Read;
                    if f.read(&mut head).unwrap_or(0) >= 2 && &head[..2] == b"MZ" {
                        anyhow::bail!(
                            "SAFETY: refused, {} is a Windows executable in disguise",
                            p.strip_prefix(root).unwrap_or(&p).display()
                        );
                    }
                }
            }
        }
        Ok(())
    }
    let mut count = 0;
    walk(root, root, 0, &mut count)?;
    Ok(count)
}

/// With `keep`, replaced folders become the backup; failing to keep them
/// doesn't fail the install.
pub fn place(zip_path: &Path, addons_dir: &Path, keep: Option<Keep>) -> anyhow::Result<Placed> {
    fs::create_dir_all(addons_dir)?;
    let work = addons_dir.join(TMP_DIR).join(unique());
    let extract_dir = work.join("x");
    let backup_dir = work.join("old");
    fs::create_dir_all(&extract_dir)?;
    fs::create_dir_all(&backup_dir)?;

    let result = (|| -> anyhow::Result<(Vec<String>, Vec<String>)> {
        let file = fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file).context("not a valid zip")?;
        archive.extract(&extract_dir).context("extract failed")?;
        let files = safety_scan(&extract_dir)?;
        crate::logi!("zip ok: {} files, no executables", files);

        let sources = addon_dirs(&extract_dir)?;
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new(); // (target, backup)
        let mut installed = Vec::new();
        let mut replaced = Vec::new();

        let restore = |moved: &Vec<(PathBuf, PathBuf)>| {
            for (target, backup) in moved.iter().rev() {
                let _ = fs::remove_dir_all(target);
                let _ = fs::rename(backup, target);
            }
        };

        for src in sources {
            let name = src
                .file_name()
                .ok_or_else(|| anyhow!("bad folder name"))?
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') || name.contains("..") {
                anyhow::bail!("refusing suspicious folder name {name}");
            }
            let target = addons_dir.join(&name);
            let backup = backup_dir.join(&name);
            if target.exists() {
                if let Err(e) = fs::rename(&target, &backup) {
                    restore(&moved);
                    return Err(anyhow!("could not move aside {name}: {e} (is WoW running?)"));
                }
                moved.push((target.clone(), backup));
                replaced.push(name.clone());
            } else {
                moved.push((target.clone(), backup));
            }
            if let Err(e) = fs::rename(&src, &target) {
                restore(&moved);
                return Err(anyhow!("could not install {name}: {e}"));
            }
            installed.push(name);
        }
        Ok((installed, replaced))
    })();

    let result = result.map(|(folders, replaced)| {
        let kept = match keep.filter(|_| !replaced.is_empty()) {
            Some(k) => {
                let b = Backup { key: k.key, version: k.version, record: k.record, folders: replaced, current: folders.clone(), saved_at: crate::state::now_secs() };
                keep_old(addons_dir, &backup_dir, &b).map_err(|e| crate::loge!("keep {}: {e}", b.key)).is_ok()
            }
            None => false,
        };
        Placed { folders, kept }
    });
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_file(zip_path);
    let _ = fs::remove_dir(addons_dir.join(TMP_DIR));
    result
}

fn safe_name(f: &str) -> bool {
    !f.is_empty() && !f.starts_with('.') && !f.contains("..") && !f.contains(['/', '\\'])
}

fn slug(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' }).collect()
}

pub fn backup_path(addons_dir: &Path, key: &str) -> PathBuf {
    addons_dir.join(BACKUP_DIR).join(slug(key))
}

fn write_manifest(dir: &Path, b: &Backup) -> anyhow::Result<()> {
    let tmp = dir.join(format!("{MANIFEST}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(b)?)?;
    fs::rename(&tmp, dir.join(MANIFEST))?;
    Ok(())
}

fn keep_old(addons_dir: &Path, old: &Path, b: &Backup) -> anyhow::Result<()> {
    let dest = backup_path(addons_dir, &b.key);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(addons_dir.join(BACKUP_DIR))?;
    fs::rename(old, &dest)?;
    write_manifest(&dest, b)
}

/// None unless every folder the manifest lists is present.
fn read_dir_backup(dir: &Path) -> Option<Backup> {
    let b: Backup = serde_json::from_slice(&fs::read(dir.join(MANIFEST)).ok()?).ok()?;
    let ok = !b.folders.is_empty()
        && b.folders.iter().chain(&b.current).all(|f| safe_name(f))
        && b.folders.iter().all(|f| dir.join(f).is_dir());
    ok.then_some(b)
}

pub fn read_backup(addons_dir: &Path, key: &str) -> Option<Backup> {
    read_dir_backup(&backup_path(addons_dir, key)).filter(|b| b.key == key)
}

pub fn read_backups(addons_dir: &Path) -> HashMap<String, Backup> {
    let Ok(rd) = fs::read_dir(addons_dir.join(BACKUP_DIR)) else { return HashMap::new() };
    rd.flatten()
        .filter_map(|e| read_dir_backup(&e.path()).filter(|b| e.file_name().to_string_lossy() == slug(&b.key)))
        .map(|b| (b.key.clone(), b))
        .collect()
}

pub fn remove_backup(addons_dir: &Path, key: &str) {
    let _ = fs::remove_dir_all(backup_path(addons_dir, key));
}

/// The displaced version (described by `version` and `record`) becomes the
/// backup, so restoring again undoes this. All-or-nothing.
pub fn restore(addons_dir: &Path, key: &str, version: Option<String>, record: Option<Installed>) -> anyhow::Result<Backup> {
    let dir = backup_path(addons_dir, key);
    let b = read_backup(addons_dir, key).ok_or_else(|| anyhow!("There is no earlier version of this addon to go back to"))?;
    let work = addons_dir.join(TMP_DIR).join(unique());
    let out = work.join("out");
    fs::create_dir_all(&out)?;
    let mut in_the_way: Vec<&String> = Vec::new();
    for f in b.current.iter().chain(&b.folders) {
        if !in_the_way.iter().any(|g| g.eq_ignore_ascii_case(f)) {
            in_the_way.push(f);
        }
    }
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new(); // (from, to)
    let result = (|| -> anyhow::Result<Vec<String>> {
        let mut displaced = Vec::new();
        for f in in_the_way {
            let p = addons_dir.join(f);
            if fs::symlink_metadata(&p).is_ok() {
                fs::rename(&p, out.join(f)).map_err(|e| anyhow!("could not move aside {f}: {e} (is WoW running?)"))?;
                done.push((p, out.join(f)));
                displaced.push(f.clone());
            }
        }
        for f in &b.folders {
            let (from, to) = (dir.join(f), addons_dir.join(f));
            fs::rename(&from, &to).map_err(|e| anyhow!("could not put back {f}: {e}"))?;
            done.push((from, to));
        }
        Ok(displaced)
    })();
    let displaced = match result {
        Ok(d) => d,
        Err(e) => {
            for (from, to) in done.iter().rev() {
                let _ = fs::rename(to, from);
            }
            let _ = fs::remove_dir_all(&work);
            let _ = fs::remove_dir(addons_dir.join(TMP_DIR));
            return Err(e);
        }
    };
    let _ = fs::remove_file(dir.join(MANIFEST));
    let mut kept = Vec::new();
    for f in &displaced {
        match fs::rename(out.join(f), dir.join(f)) {
            Ok(()) => kept.push(f.clone()),
            Err(e) => crate::loge!("restore {key}: could not keep {f}: {e}"),
        }
    }
    if kept.len() == displaced.len() && !kept.is_empty() {
        let next = Backup { key: key.to_string(), version, record, folders: kept, current: b.folders.clone(), saved_at: crate::state::now_secs() };
        if let Err(e) = write_manifest(&dir, &next) {
            crate::loge!("restore {key}: {e}");
        }
    } else {
        remove_backup(addons_dir, key);
    }
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir(addons_dir.join(TMP_DIR));
    Ok(b)
}

pub fn remove_folders(addons_dir: &Path, folders: &[String]) -> anyhow::Result<Vec<String>> {
    let mut gone = Vec::new();
    for f in folders {
        if !safe_name(f) {
            anyhow::bail!("refusing to remove suspicious folder name {f}");
        }
        let p = addons_dir.join(f);
        if p.is_dir() {
            fs::remove_dir_all(&p).with_context(|| format!("remove {f}"))?;
            gone.push(f.clone());
        }
    }
    Ok(gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Links inside an addon folder can point at SavedVariables.
    #[test]
    #[cfg(unix)]
    fn removing_a_folder_never_follows_a_link() {
        let root = std::env::temp_dir().join(format!("af-link-{}", unique()));
        let saved = root.join("WTF").join("SavedVariables");
        fs::create_dir_all(&saved).unwrap();
        fs::write(saved.join("Addon.lua"), "keep me").unwrap();
        let addons = root.join("AddOns");
        let addon = addons.join("Addon");
        fs::create_dir_all(&addon).unwrap();
        let link = addon.join("LinkedData");
        std::os::unix::fs::symlink(&saved, &link).unwrap();
        assert!(link.join("Addon.lua").exists(), "the link should expose the file");
        let gone = remove_folders(&addons, &["Addon".to_string()]).unwrap();
        assert_eq!(gone, vec!["Addon".to_string()]);
        assert!(!addon.exists());
        assert!(saved.join("Addon.lua").exists(), "the linked SavedVariables file must survive");
        let _ = fs::remove_dir_all(&root);
    }

    fn addon_zip(dir: &Path, version: &str, folders: &[&str]) -> PathBuf {
        let path = dir.join(format!("{}-{version}.zip", unique()));
        let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for f in folders {
            z.start_file(format!("{f}/{f}.toc"), zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(format!("## Interface: 16001\n## Version: {version}\n").as_bytes()).unwrap();
        }
        z.finish().unwrap();
        path
    }

    fn toc(addons: &Path, f: &str) -> String {
        fs::read_to_string(addons.join(f).join(format!("{f}.toc"))).unwrap_or_default()
    }

    fn keep(version: &str) -> Option<Keep> {
        Some(Keep { key: "github:a/b".into(), version: Some(version.into()), record: None })
    }

    #[test]
    fn an_update_keeps_the_old_version_and_restore_swaps_them() {
        let root = std::env::temp_dir().join(format!("af-backup-{}", unique()));
        let addons = root.join("AddOns");
        fs::create_dir_all(&addons).unwrap();
        place(&addon_zip(&root, "1", &["Core", "Old"]), &addons, None).unwrap();
        assert!(read_backup(&addons, "github:a/b").is_none(), "a fresh install keeps nothing");
        let placed = place(&addon_zip(&root, "2", &["Core", "New"]), &addons, keep("1")).unwrap();
        assert!(placed.kept);
        let b = read_backup(&addons, "github:a/b").unwrap();
        assert_eq!((b.version.as_deref(), b.folders.clone(), b.current.clone()), (Some("1"), vec!["Core".to_string()], vec!["Core".to_string(), "New".to_string()]));
        assert!(read_backups(&addons).contains_key("github:a/b"));
        assert!(toc(&addons, "Core").contains("Version: 2"));

        let rec = Installed { source: "github".into(), source_id: "a/b".into(), version: "2".into(), folders: vec!["Core".into(), "New".into()], installed_at: 1 };
        let back = restore(&addons, "github:a/b", Some("2".into()), Some(rec)).unwrap();
        assert_eq!(back.version.as_deref(), Some("1"));
        assert!(back.record.is_none(), "v1 wasn't installed by AddonForge");
        assert!(toc(&addons, "Core").contains("Version: 1"));
        assert!(!addons.join("New").exists(), "v1 didn't have New");
        assert!(addons.join("Old").exists());
        let b = read_backup(&addons, "github:a/b").unwrap();
        assert_eq!((b.version.as_deref(), b.record.as_ref().map(|r| r.version.as_str())), (Some("2"), Some("2")));

        restore(&addons, "github:a/b", Some("1".into()), None).unwrap();
        assert!(toc(&addons, "Core").contains("Version: 2") && toc(&addons, "New").contains("Version: 2"));
        assert_eq!(read_backup(&addons, "github:a/b").unwrap().version.as_deref(), Some("1"));
        assert!(!addons.join(TMP_DIR).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn a_failed_restore_puts_everything_back() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("af-restore-{}", unique()));
        let addons = root.join("AddOns");
        fs::create_dir_all(&addons).unwrap();
        place(&addon_zip(&root, "1", &["Core"]), &addons, None).unwrap();
        place(&addon_zip(&root, "2", &["Core"]), &addons, keep("1")).unwrap();
        let dir = backup_path(&addons, "github:a/b");
        let mut b = read_backup(&addons, "github:a/b").unwrap();
        b.folders.push("Late".into());
        write_manifest(&dir, &b).unwrap();
        assert!(read_backup(&addons, "github:a/b").is_none());
        assert!(restore(&addons, "github:a/b", None, None).is_err());
        // Late can't move (its ".." isn't writable) after Core has.
        fs::create_dir_all(dir.join("Late")).unwrap();
        fs::write(addons.join("Late"), "in the way").unwrap();
        fs::set_permissions(dir.join("Late"), fs::Permissions::from_mode(0o555)).unwrap();
        let err = restore(&addons, "github:a/b", None, None).unwrap_err();
        fs::set_permissions(dir.join("Late"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(err.to_string().contains("Late"), "{err}");
        assert!(toc(&addons, "Core").contains("Version: 2"));
        assert_eq!(fs::read_to_string(addons.join("Late")).unwrap(), "in the way");
        assert!(toc(&dir, "Core").contains("Version: 1"));
        assert!(read_backup(&addons, "github:a/b").is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn safety_scan_refuses_executables() {
        let root = std::env::temp_dir().join(format!("af-safety-{}", unique()));
        let addon = root.join("Evil");
        fs::create_dir_all(&addon).unwrap();
        fs::write(addon.join("Evil.toc"), "## Interface: 16001\n").unwrap();
        fs::write(addon.join("Evil.lua"), "print(1)\n").unwrap();
        assert!(safety_scan(&root).is_ok());
        fs::write(addon.join("helper.exe"), b"MZ\0\0").unwrap();
        let err = safety_scan(&root).unwrap_err().to_string();
        assert!(err.contains("SAFETY"), "{err}");
        fs::remove_file(addon.join("helper.exe")).unwrap();
        fs::write(addon.join("README"), b"MZ\x90\x00").unwrap();
        assert!(safety_scan(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
