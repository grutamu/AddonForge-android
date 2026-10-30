use anyhow::{anyhow, Context};
use serde::Serialize;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Copies of the game's WTF folder (SavedVariables, keybinds, macros), kept in
/// the app's private dir so they outlive a game reinstall.
pub const DIR: &str = "settings-backups";
/// Kept per kind: manual ones never push out automatic ones, or the reverse.
const KEEP: usize = 5;
/// Inside the product folder so the swap is a same-volume rename.
const TMP_DIR: &str = ".addonforge-wtf-tmp";
const REASONS: &[&str] = &["manual", "update", "restore"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SettingsBackup {
    pub id: String,
    pub saved_at: u64,
    /// manual | update (before Update all) | restore (before a restore)
    pub reason: String,
    pub size: u64,
}

pub fn wtf_dir(install: &Path) -> PathBuf {
    install.join("WTF")
}

fn parse_id(id: &str) -> Option<(u64, &str)> {
    let rest = id.strip_prefix("wtf-")?.strip_suffix(".zip")?;
    let (secs, reason) = rest.split_once('-')?;
    let secs = secs.parse().ok()?;
    REASONS.contains(&reason).then_some((secs, reason))
}

/// Newest first.
pub fn list(dir: &Path) -> Vec<SettingsBackup> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<SettingsBackup> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let (saved_at, reason) = parse_id(&id)?;
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            Some(SettingsBackup { reason: reason.into(), id, saved_at, size })
        })
        .collect();
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then(b.id.cmp(&a.id)));
    out
}

/// Regular files under `root` as (relative path with `/`, full path), sorted.
/// Links are skipped, never followed.
fn files(root: &Path) -> anyhow::Result<Vec<(String, PathBuf)>> {
    fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<(String, PathBuf)>) -> anyhow::Result<()> {
        if depth > 32 {
            anyhow::bail!("WTF nests folders absurdly deep");
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let ft = e.file_type()?;
            let name = e.file_name().to_string_lossy().to_string();
            let r = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            if ft.is_dir() {
                walk(&e.path(), &r, depth + 1, out)?;
            } else if ft.is_file() {
                out.push((r, e.path()));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, "", 0, &mut out)?;
    Ok(out)
}

/// Names, sizes and modified times: equal when nothing was saved since.
fn fingerprint(list: &[(String, PathBuf)]) -> String {
    let mut h = std::hash::DefaultHasher::new();
    for (rel, p) in list {
        rel.hash(&mut h);
        if let Ok(m) = fs::metadata(p) {
            m.len().hash(&mut h);
            m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).hash(&mut h);
        }
    }
    format!("addonforge-wtf {:016x}", h.finish())
}

fn comment(path: &Path) -> Option<String> {
    let z = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    Some(String::from_utf8_lossy(z.comment()).to_string())
}

/// Removes all but the newest `KEEP` of each kind.
fn prune(dir: &Path) {
    let all = list(dir);
    for reason in REASONS {
        for b in all.iter().filter(|b| b.reason == *reason).skip(KEEP) {
            let _ = fs::remove_file(dir.join(&b.id));
        }
    }
}

/// Zips `install`/WTF into `dir`. With `skip_unchanged`, returns the newest
/// backup instead when WTF hasn't changed since it was made.
pub fn backup(install: &Path, dir: &Path, reason: &str, skip_unchanged: bool) -> anyhow::Result<SettingsBackup> {
    if !REASONS.contains(&reason) {
        anyhow::bail!("unknown backup reason {reason}");
    }
    let wtf = wtf_dir(install);
    if !wtf.is_dir() {
        anyhow::bail!("There's no WTF folder yet. Play once so the game creates it.");
    }
    let list_now = files(&wtf)?;
    if list_now.is_empty() {
        anyhow::bail!("The WTF folder is empty, so there's nothing to back up");
    }
    let print = fingerprint(&list_now);
    if skip_unchanged {
        if let Some(newest) = list(dir).into_iter().next() {
            if comment(&dir.join(&newest.id)).as_deref() == Some(print.as_str()) {
                return Ok(newest);
            }
        }
    }
    fs::create_dir_all(dir)?;
    let mut secs = crate::state::now_secs();
    while list(dir).iter().any(|b| b.saved_at == secs) {
        secs += 1;
    }
    let id = format!("wtf-{secs}-{reason}.zip");
    let tmp = dir.join(format!("{id}.tmp"));
    let result = (|| -> anyhow::Result<()> {
        let mut z = zip::ZipWriter::new(fs::File::create(&tmp)?);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        let mut buf = Vec::new();
        for (rel, p) in &list_now {
            buf.clear();
            fs::File::open(p).and_then(|mut f| f.read_to_end(&mut buf)).with_context(|| format!("read WTF/{rel}"))?;
            z.start_file(format!("WTF/{rel}"), opts)?;
            z.write_all(&buf)?;
        }
        z.set_comment(print.clone());
        z.finish()?.sync_all()?;
        fs::rename(&tmp, dir.join(&id))?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    prune(dir);
    let size = fs::metadata(dir.join(&id)).map(|m| m.len()).unwrap_or(0);
    Ok(SettingsBackup { id, saved_at: secs, reason: reason.into(), size })
}

/// Unpacks `WTF/...` entries only; refuses links and paths that escape.
fn extract(zip_path: &Path, dest: &Path) -> anyhow::Result<usize> {
    let mut z = zip::ZipArchive::new(fs::File::open(zip_path)?).context("not a valid zip")?;
    let mut n = 0;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        if f.is_symlink() {
            anyhow::bail!("backup contains a link: {}", f.name());
        }
        let rel = f
            .enclosed_name()
            .and_then(|p| p.strip_prefix("WTF").ok().map(Path::to_path_buf))
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| anyhow!("backup has an unexpected entry: {}", f.name()))?;
        let out = dest.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut f, &mut fs::File::create(&out)?)?;
        n += 1;
    }
    if n == 0 {
        anyhow::bail!("backup is empty");
    }
    Ok(n)
}

/// Replaces `install`/WTF with backup `id`, all or nothing. The current WTF is
/// backed up first (reason "restore"), so a restore can be undone; that backup
/// is returned.
pub fn restore(install: &Path, dir: &Path, id: &str) -> anyhow::Result<Option<SettingsBackup>> {
    if parse_id(id).is_none() || id.contains(['/', '\\']) {
        anyhow::bail!("unknown backup {id}");
    }
    let zip_path = dir.join(id);
    if !zip_path.is_file() {
        anyhow::bail!("That backup is gone");
    }
    let wtf = wtf_dir(install);
    let work = install.join(TMP_DIR).join(format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    let fresh = work.join("WTF");
    let old = work.join("old");
    let result = (|| -> anyhow::Result<Option<SettingsBackup>> {
        fs::create_dir_all(&fresh)?;
        // Before the undo backup, whose pruning may remove this one.
        extract(&zip_path, &fresh)?;
        let undo = match backup(install, dir, "restore", true) {
            Ok(b) => Some(b),
            Err(_) if !wtf.exists() || files(&wtf).map(|f| f.is_empty()).unwrap_or(false) => None,
            Err(e) => return Err(e.context("couldn't back up the current settings first, so nothing was changed")),
        };
        if fs::symlink_metadata(&wtf).is_ok() {
            fs::rename(&wtf, &old).map_err(|e| anyhow!("could not move the current WTF aside: {e} (is WoW running?)"))?;
        }
        if let Err(e) = fs::rename(&fresh, &wtf) {
            if fs::symlink_metadata(&old).is_ok() {
                let _ = fs::rename(&old, &wtf);
            }
            return Err(anyhow!("could not put the backup in place: {e}"));
        }
        Ok(undo)
    })();
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir(install.join(TMP_DIR));
    result
}

pub fn clean_tmp(install: &Path) {
    let _ = fs::remove_dir_all(install.join(TMP_DIR));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("af-wtf-{name}-{n:x}"))
    }

    fn write(p: &Path, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn backup_and_restore_round_trip_and_restore_can_be_undone() {
        let root = tmp("round");
        let install = root.join("_forever_");
        let store = root.join("store");
        let sv = wtf_dir(&install).join("Account").join("ME").join("SavedVariables");
        write(&sv.join("Questie.lua"), "QuestieConfig = { v = 1 }");
        write(&wtf_dir(&install).join("Config.wtf"), "SET locale \"enUS\"");

        let first = backup(&install, &store, "manual", false).unwrap();
        assert_eq!(first.reason, "manual");
        assert_eq!(list(&store), vec![first.clone()]);
        let same = backup(&install, &store, "update", true).unwrap();
        assert_eq!(same, first, "unchanged WTF reuses the newest backup");

        write(&sv.join("Questie.lua"), "QuestieConfig = { v = 2, broken = true }");
        write(&sv.join("New.lua"), "added after the backup");
        let undo = restore(&install, &store, &first.id).unwrap().expect("current settings saved first");
        assert_eq!(undo.reason, "restore");
        assert_eq!(fs::read_to_string(sv.join("Questie.lua")).unwrap(), "QuestieConfig = { v = 1 }");
        assert!(!sv.join("New.lua").exists(), "restore replaces the whole folder");
        assert!(wtf_dir(&install).join("Config.wtf").is_file());
        assert!(!install.join(TMP_DIR).exists());

        restore(&install, &store, &undo.id).unwrap();
        assert!(fs::read_to_string(sv.join("Questie.lua")).unwrap().contains("broken"));
        assert!(sv.join("New.lua").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_wtf_is_an_error_and_restoring_creates_it() {
        let root = tmp("none");
        let install = root.join("_forever_");
        let store = root.join("store");
        write(&wtf_dir(&install).join("Config.wtf"), "x");
        let b = backup(&install, &store, "manual", false).unwrap();
        fs::remove_dir_all(wtf_dir(&install)).unwrap();
        assert!(backup(&install, &store, "manual", false).unwrap_err().to_string().contains("no WTF"));
        assert_eq!(restore(&install, &store, &b.id).unwrap(), None);
        assert_eq!(fs::read_to_string(wtf_dir(&install).join("Config.wtf")).unwrap(), "x");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn old_backups_are_pruned_per_kind() {
        let root = tmp("prune");
        let install = root.join("_forever_");
        let store = root.join("store");
        write(&wtf_dir(&install).join("Config.wtf"), "x");
        let manual = backup(&install, &store, "manual", false).unwrap();
        for _ in 0..KEEP + 2 {
            backup(&install, &store, "update", false).unwrap();
        }
        let all = list(&store);
        assert_eq!(all.iter().filter(|b| b.reason == "update").count(), KEEP);
        assert!(all.contains(&manual), "automatic backups never push out a manual one");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_refuses_bad_ids_and_hostile_zips() {
        let root = tmp("hostile");
        let install = root.join("_forever_");
        let store = root.join("store");
        write(&wtf_dir(&install).join("Config.wtf"), "mine");
        for id in ["../state.json", "wtf-1-manual.zip/../../x", "state.json", "wtf-x-manual.zip"] {
            assert!(restore(&install, &store, id).is_err(), "{id}");
        }
        fs::create_dir_all(&store).unwrap();
        let evil = store.join("wtf-5-manual.zip");
        let mut z = zip::ZipWriter::new(fs::File::create(&evil).unwrap());
        z.start_file("WTF/ok.lua", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"fine").unwrap();
        z.start_file("Interface/AddOns/Evil/Evil.lua", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"not settings").unwrap();
        z.finish().unwrap();
        assert!(restore(&install, &store, "wtf-5-manual.zip").unwrap_err().to_string().contains("unexpected"));
        assert_eq!(fs::read_to_string(wtf_dir(&install).join("Config.wtf")).unwrap(), "mine");
        assert!(!install.join("Interface").exists());
        assert!(!install.join(TMP_DIR).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)]
    fn backup_skips_links_instead_of_following_them() {
        let root = tmp("link");
        let install = root.join("_forever_");
        let store = root.join("store");
        write(&root.join("outside").join("secret.txt"), "not WoW's");
        write(&wtf_dir(&install).join("Config.wtf"), "x");
        std::os::unix::fs::symlink(root.join("outside"), wtf_dir(&install).join("Linked")).unwrap();
        let b = backup(&install, &store, "manual", false).unwrap();
        let z = zip::ZipArchive::new(fs::File::open(store.join(&b.id)).unwrap()).unwrap();
        let names: Vec<_> = z.file_names().map(String::from).collect();
        assert_eq!(names, vec!["WTF/Config.wtf".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }
}
