// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent settings, stored in `~/.config/balota/config.json`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            player_name: "Survivor".to_string(),
            custom_steam_root: None,
            extra_launch_args: String::new(),
            favorites: Vec::new(),
            history: Vec::new(),
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("balota");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("config.json"))
}

pub fn load() -> AppConfig {
    let Some(path) = config_path() else {
        return AppConfig::default();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return AppConfig::default();
    };

    // A corrupt file must not stop the app from starting: ignore it and let
    // the next save overwrite it.
    serde_json::from_str(&content).unwrap_or_default()
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path().ok_or("Could not determine the configuration directory")?;
    let body = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;

    // Atomic write: losing power mid-save leaves the old file untouched.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| format!("Could not save settings: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Could not save settings: {e}"))?;

    Ok(())
}

impl AppConfig {
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
    fn favorites_toggle() {
        let mut config = AppConfig::default();
        assert!(config.toggle_favorite("1.2.3.4:27016"));
        assert_eq!(config.favorites.len(), 1);
        assert!(!config.toggle_favorite("1.2.3.4:27016"));
        assert!(config.favorites.is_empty());
    }

    #[test]
    fn a_corrupt_config_does_not_break_startup() {
        let parsed: AppConfig = serde_json::from_str("{ not json at all }").unwrap_or_default();
        assert_eq!(parsed.player_name, "Survivor");
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: AppConfig = serde_json::from_str(r#"{"playerName":"Ayoze"}"#).unwrap();
        assert_eq!(parsed.player_name, "Ayoze");
        assert!(parsed.favorites.is_empty());
        assert_eq!(parsed.extra_launch_args, "");
    }
}
