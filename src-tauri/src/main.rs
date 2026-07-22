// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod dzsa;
mod steam;

use config::{load_config, save_config, AppUserConfig};
use dzsa::{ping_server_a2s, query_dzsa_server, DzsaApiResponse, ServerPingResult};
use steam::{detect_steam_env, get_installed_mods, launch_dayz, prepare_mod_symlinks, InstalledModInfo, SteamEnvironment};

#[tauri::command]
fn get_steam_environment() -> SteamEnvironment {
    detect_steam_env()
}

#[tauri::command]
fn get_installed_workshop_mods() -> Vec<InstalledModInfo> {
    let env = detect_steam_env();
    get_installed_mods(&env.workshop_dir)
}

#[tauri::command]
async fn query_dzsa(ip: String, query_port: u16) -> Result<DzsaApiResponse, String> {
    query_dzsa_server(&ip, query_port).await
}

#[tauri::command]
async fn ping_a2s(ip: String, query_port: u16) -> ServerPingResult {
    ping_server_a2s(&ip, query_port).await
}

#[tauri::command]
fn launch_game(
    ip_port: Option<String>,
    mod_ids: Vec<String>,
    custom_name: Option<String>,
    use_gamemode: bool,
    use_mangohud: bool,
    custom_args: Option<String>,
) -> Result<String, String> {
    let env = detect_steam_env();
    if !env.dayz_found {
        return Err("DayZ installation directory not detected on system!".to_string());
    }

    // 1. Prepare symlinks for mods
    let symlinks = prepare_mod_symlinks(&env.dayz_dir, &env.workshop_dir, &mod_ids)?;

    // 2. Launch DayZ via Steam
    launch_dayz(
        env.is_flatpak,
        ip_port.as_deref(),
        custom_name.as_deref(),
        &symlinks,
        use_gamemode,
        use_mangohud,
        custom_args.as_deref(),
    )
}

#[tauri::command]
fn get_app_config() -> AppUserConfig {
    load_config()
}

#[tauri::command]
fn save_app_config(config: AppUserConfig) -> Result<(), String> {
    save_config(&config)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_steam_environment,
            get_installed_workshop_mods,
            query_dzsa,
            ping_a2s,
            launch_game,
            get_app_config,
            save_app_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
