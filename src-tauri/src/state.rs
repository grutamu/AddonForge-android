//! Persistent local state: `state.json` in the app's private files folder.
//!
//! This is the only settings file the app writes outside the game's AddOns
//! folder (plus `addonforge.log` beside it). Written atomically (tmp +
//! rename) so a crash can never corrupt it.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Installed {
    /// Source kind: github | wago | wowi | tukui | curse
    pub source: String,
    pub source_id: String,
    pub version: String,
    pub folders: Vec<String>,
    pub installed_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppState {
    /// Product folder we manage, e.g. `/storage/emulated/0/WoW Forever/_classic_beta_`.
    #[serde(default)]
    pub install_path: Option<String>,
    /// Optional user-supplied Wago Addons API key (stored locally only).
    #[serde(default)]
    pub wago_key: Option<String>,
    /// Packages we installed ourselves, keyed by package key.
    #[serde(default)]
    pub installed: HashMap<String, Installed>,
    /// Consider GitHub pre-releases and Wago beta builds when checking.
    #[serde(default)]
    pub allow_prerelease: bool,
    /// Package keys whose updates are held back.
    #[serde(default)]
    pub pinned: BTreeSet<String>,
    /// Package keys hidden from the Installed list.
    #[serde(default)]
    pub ignored: BTreeSet<String>,
    /// A self-update version the user chose to skip ("v0.2.0-b7").
    #[serde(default)]
    pub skip_self_update: Option<String>,
    /// Build number of the last version that ran, for the one-time "what's new" note.
    #[serde(default)]
    pub last_seen_build: u32,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl AppState {
    /// The app's private files folder on Android. `ADDONFORGE_STATE_DIR`
    /// overrides it so tests and screenshots can use a throwaway profile.
    pub fn dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("ADDONFORGE_STATE_DIR").filter(|v| !v.is_empty()) {
            return PathBuf::from(dir);
        }
        #[cfg(target_os = "android")]
        {
            // No config dir on Android: use the app's private files folder,
            // found from the package name the process runs under.
            let cmdline = std::fs::read("/proc/self/cmdline").unwrap_or_default();
            let pkg = String::from_utf8_lossy(cmdline.split(|b| *b == 0).next().unwrap_or_default()).to_string();
            return PathBuf::from(format!("/data/data/{}/files", pkg.split(':').next().unwrap_or_default())).join("AddonForge");
        }
        // The Mac preview and `cargo test` get a throwaway profile, never a
        // real one.
        #[cfg(not(target_os = "android"))]
        std::env::temp_dir().join("AddonForge-host")
    }

    pub fn path() -> PathBuf {
        Self::dir().join("state.json")
    }

    pub fn load() -> Self {
        match fs::read(Self::path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::dir();
        fs::create_dir_all(&dir)?;
        let tmp = dir.join("state.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        let final_path = Self::path();
        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(&tmp, &final_path)?;
        Ok(())
    }

    pub fn has_wago_key(&self) -> bool {
        self.wago_key.as_deref().map(|k| !k.trim().is_empty()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_state_with_removed_fields_still_loads() {
        for v in ["true", "false"] {
            let json = format!(r#"{{"install_path":"/x","allow_prerelease":true,"curse_unofficial":{v},"pinned":["a"]}}"#);
            let st: AppState = serde_json::from_str(&json).unwrap();
            assert_eq!(st.install_path.as_deref(), Some("/x"));
            assert!(st.allow_prerelease);
            assert!(st.pinned.contains("a"));
            assert!(!serde_json::to_string(&st).unwrap().contains("curse_unofficial"));
        }
    }
}
