use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppUserConfig {
    pub player_name: String,
    pub is_flatpak: bool,
    pub custom_steam_root: Option<String>,
    pub use_gamemode: bool,
    pub use_mangohud: bool,
    pub custom_launch_params: String,
    pub favorites: Vec<String>,
}

impl Default for AppUserConfig {
    fn default() -> Self {
        Self {
            player_name: "Survivor".to_string(),
            is_flatpak: false,
            custom_steam_root: None,
            use_gamemode: false,
            use_mangohud: false,
            custom_launch_params: "-nolauncher -world=empty".to_string(),
            favorites: Vec::new(),
        }
    }
}

pub fn get_config_path() -> PathBuf {
    let home = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    let dir = home.join("apexdz");
    fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

pub fn load_config() -> AppUserConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(cfg) = serde_json::from_str(&content) {
                return cfg;
            }
        }
    }
    AppUserConfig::default()
}

pub fn save_config(cfg: &AppUserConfig) -> Result<(), String> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}
