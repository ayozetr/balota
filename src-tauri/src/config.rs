// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent settings, stored in `~/.config/balota/config.json`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_HISTORY: usize = 40;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub name: String,
    pub map: String,
    /// Seconds since the Unix epoch.
    pub last_played: u64,
    pub times_played: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilters {
    pub search: String,
    pub map: Option<String>,
    pub mods: Option<String>,
    pub perspective: Option<String>,
    pub hide_empty: bool,
    pub hide_full: bool,
    pub hide_password: bool,
    pub sort: Option<String>,
    pub sort_desc: Option<bool>,
    pub tab: Option<String>,
}

/// Note the empty `player_name`: DayZ's own placeholder is deliberately *not*
/// a default here, because a good number of servers kick it on sight. The
/// interface fills it in from Steam, or asks.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub player_name: String,
    /// Steam path set by hand when auto-detection comes up empty.
    pub custom_steam_root: Option<String>,
    /// Extra parameters appended to the launch command line.
    pub extra_launch_args: String,
    /// Favourite server IDs (`ip:query_port`).
    pub favorites: Vec<String>,
    pub history: Vec<HistoryEntry>,
    /// Last used filters, so the list comes back the way it was left.
    pub filters: SavedFilters,
}

/// The fields the Settings tab is allowed to change.
///
/// The interface used to send back the whole `AppConfig`, which meant a
/// keystroke in "in-game name" carried a stale copy of `favorites` and
/// `history` with it — and quietly undid whatever had changed in between.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub player_name: Option<String>,
    /// `Some(None)` clears it; `None` leaves it alone.
    ///
    /// Needs `double_option` because serde maps both an absent key and an
    /// explicit `null` to `None`, and here those mean opposite things:
    /// "don't touch the Steam path" versus "go back to auto-detection".
    #[serde(default, deserialize_with = "double_option")]
    pub custom_steam_root: Option<Option<String>>,
    pub extra_launch_args: Option<String>,
    pub filters: Option<SavedFilters>,
}

fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

fn config_dir() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("balota");
    fs::create_dir_all(&dir).ok()?;
    // History, favourites and IPs are nobody else's business.
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    Some(dir)
}

fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("config.json"))
}

pub fn load() -> AppConfig {
    let Some(path) = config_path() else {
        return AppConfig::default();
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return AppConfig::default();
    };

    match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            // Losing favourites and history in silence is worse than the
            // corruption itself: keep the file so it can be recovered by hand.
            let backup = path.with_extension("json.broken");
            let _ = fs::rename(&path, &backup);
            eprintln!(
                "balota: {} could not be read ({e}); it was moved to {} and defaults were used.",
                path.display(),
                backup.display()
            );
            AppConfig::default()
        }
    }
}

/// Writes the settings atomically and durably.
///
/// The temporary file is per-process and per-call, and opened with `O_EXCL`:
/// the old fixed name could be pre-created as a symlink by anything running as
/// this user, and two saves racing each other could rename a half-written file
/// over a good one. `sync_all` is what actually makes the "losing power leaves
/// the old file untouched" promise true.
pub fn save(config: &AppConfig) -> Result<(), String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let path = config_path().ok_or("Could not determine the configuration directory")?;
    let dir = path.parent().ok_or("Bad configuration path")?;
    let body = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;

    let tmp = dir.join(format!(
        "config.json.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let write = || -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &path)
    };

    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Could not save settings: {e}")
    })
}

impl AppConfig {
    /// Applies the fields the Settings tab owns, leaving everything else be.
    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(name) = patch.player_name {
            self.player_name = name;
        }
        if let Some(root) = patch.custom_steam_root {
            self.custom_steam_root = root.filter(|s| !s.trim().is_empty());
        }
        if let Some(args) = patch.extra_launch_args {
            self.extra_launch_args = args;
        }
        if let Some(filters) = patch.filters {
            self.filters = filters;
        }
    }

    pub fn toggle_favorite(&mut self, id: &str) -> bool {
        match self.favorites.iter().position(|f| f == id) {
            Some(index) => {
                self.favorites.remove(index);
                false
            }
            None => {
                self.favorites.push(id.to_string());
                true
            }
        }
    }

    /// Records a session in the history, updating the entry if it already
    /// exists.
    pub fn record_launch(&mut self, id: &str, name: &str, map: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Some(entry) = self.history.iter_mut().find(|e| e.id == id) {
            entry.last_played = now;
            entry.times_played += 1;
            entry.name = name.to_string();
            entry.map = map.to_string();
        } else {
            self.history.push(HistoryEntry {
                id: id.to_string(),
                name: name.to_string(),
                map: map.to_string(),
                last_played: now,
                times_played: 1,
            });
        }

        // Most recent first.
        self.history
            .sort_by_key(|entry| std::cmp::Reverse(entry.last_played));
        self.history.truncate(MAX_HISTORY);
    }

    pub fn forget(&mut self, id: &str) {
        self.history.retain(|entry| entry.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_does_not_grow_without_bound() {
        let mut config = AppConfig::default();
        for i in 0..(MAX_HISTORY + 15) {
            config.record_launch(&format!("10.0.0.{i}:27016"), "Server", "Chernarus");
        }
        assert_eq!(config.history.len(), MAX_HISTORY);
    }

    #[test]
    fn replaying_a_server_updates_instead_of_duplicating() {
        let mut config = AppConfig::default();
        config.record_launch("1.2.3.4:27016", "Old name", "Chernarus");
        config.record_launch("1.2.3.4:27016", "New name", "Livonia");

        assert_eq!(config.history.len(), 1);
        assert_eq!(config.history[0].times_played, 2);
        assert_eq!(config.history[0].name, "New name");
        assert_eq!(config.history[0].map, "Livonia");
    }

    #[test]
    fn history_entries_can_be_forgotten() {
        let mut config = AppConfig::default();
        config.record_launch("1.2.3.4:27016", "Server", "Chernarus");
        config.record_launch("5.6.7.8:27016", "Other", "Livonia");

        config.forget("1.2.3.4:27016");
        assert_eq!(config.history.len(), 1);
        assert_eq!(config.history[0].id, "5.6.7.8:27016");
    }

    #[test]
    fn favorites_toggle() {
        let mut config = AppConfig::default();
        assert!(config.toggle_favorite("1.2.3.4:27016"));
        assert_eq!(config.favorites.len(), 1);
        assert!(!config.toggle_favorite("1.2.3.4:27016"));
        assert!(config.favorites.is_empty());
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: AppConfig = serde_json::from_str(r#"{"playerName":"Ayoze"}"#).unwrap();
        assert_eq!(parsed.player_name, "Ayoze");
        assert!(parsed.favorites.is_empty());
        assert_eq!(parsed.extra_launch_args, "");
    }

    #[test]
    fn there_is_no_default_player_name() {
        // A default of "Survivor" is what got players kicked; the interface
        // asks for a name instead.
        assert_eq!(AppConfig::default().player_name, "");
    }

    /// A patch must never carry the collections with it: that is how a
    /// keystroke in Settings used to wipe a favourite marked a moment earlier.
    #[test]
    fn a_settings_patch_leaves_favourites_and_history_alone() {
        let mut config = AppConfig::default();
        config.toggle_favorite("1.2.3.4:27016");
        config.record_launch("1.2.3.4:27016", "Server", "Chernarus");

        let patch: SettingsPatch = serde_json::from_str(r#"{"playerName":"Ayoze"}"#).unwrap();
        config.apply(patch);

        assert_eq!(config.player_name, "Ayoze");
        assert_eq!(config.favorites.len(), 1, "favourite survived");
        assert_eq!(config.history.len(), 1, "history survived");
    }

    #[test]
    fn a_patch_can_clear_the_custom_steam_path() {
        let mut config = AppConfig {
            custom_steam_root: Some("/somewhere".into()),
            ..Default::default()
        };

        config.apply(serde_json::from_str(r#"{"customSteamRoot":null}"#).unwrap());
        assert_eq!(config.custom_steam_root, None);

        // An absent key is not the same as an explicit null.
        let mut config = AppConfig {
            custom_steam_root: Some("/somewhere".into()),
            ..Default::default()
        };
        config.apply(serde_json::from_str(r#"{"playerName":"Ayoze"}"#).unwrap());
        assert_eq!(config.custom_steam_root.as_deref(), Some("/somewhere"));
    }
}
