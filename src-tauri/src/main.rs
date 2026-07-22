// Keeps an extra console from opening on Windows. We only support Linux, but
// the attribute is harmless.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod a2s;
mod config;
mod dzsa;
mod servers;
mod steam;
mod vdf;

use std::path::Path;

use serde::Serialize;
use tauri::{Manager, State};
use tokio::sync::{Mutex, RwLock};

use config::AppConfig;
use servers::{ListStatus, MapCount, ServerFilter, ServerPage, ServerRow, ServerStore};
use steam::{InstalledMod, LaunchOutcome, LaunchRequest, SteamEnvironment};

struct AppState {
    store: RwLock<ServerStore>,
    env: RwLock<Option<SteamEnvironment>>,
    config: Mutex<AppConfig>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModStatus {
    id: u64,
    name: String,
    installed: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ServerDetails {
    server: ServerRow,
    /// `ip:port` string ready for `-connect=`.
    connect: String,
    mods: Vec<ModStatus>,
    missing_count: usize,
    /// `true` when the data comes from a live lookup rather than the cache.
    live: bool,
    /// Why the live lookup could not be done, when that happens.
    warning: Option<String>,
}

// ---------------------------------------------------------------------------
// Steam environment
// ---------------------------------------------------------------------------

async fn current_env(state: &AppState) -> SteamEnvironment {
    if let Some(env) = state.env.read().await.clone() {
        return env;
    }
    refresh_env(state).await
}

async fn refresh_env(state: &AppState) -> SteamEnvironment {
    let custom_root = state.config.lock().await.custom_steam_root.clone();
    let env = tokio::task::spawn_blocking(move || steam::detect(custom_root.as_deref()))
        .await
        .unwrap_or_else(|_| steam::detect(None));

    *state.env.write().await = Some(env.clone());
    env
}

#[tauri::command]
async fn steam_environment(
    state: State<'_, AppState>,
    refresh: bool,
) -> Result<SteamEnvironment, String> {
    Ok(if refresh {
        refresh_env(&state).await
    } else {
        current_env(&state).await
    })
}

// ---------------------------------------------------------------------------
// Server list
// ---------------------------------------------------------------------------

#[tauri::command]
async fn refresh_servers(state: State<'_, AppState>, force: bool) -> Result<ListStatus, String> {
    if !force {
        // The disk cache saves a 3 MB download on every start.
        let mut store = state.store.write().await;
        if let Some(status) = store.load_from_cache() {
            return Ok(status);
        }
    }

    // The download happens outside the lock so queries are not blocked while
    // 26 MB come down the wire.
    let (list, body) = dzsa::fetch_master_list().await?;
    let mut store = state.store.write().await;
    Ok(store.replace(list, &body))
}

#[tauri::command]
async fn query_servers(
    state: State<'_, AppState>,
    filter: ServerFilter,
) -> Result<ServerPage, String> {
    Ok(state.store.read().await.query(&filter))
}

#[tauri::command]
async fn server_maps(state: State<'_, AppState>) -> Result<Vec<MapCount>, String> {
    Ok(state.store.read().await.maps())
}

/// Details for one server, by `ip:query_port`.
///
/// The ID does not have to be in the master list: history entries and direct
/// connections resolve through a live API lookup instead.
#[tauri::command]
async fn server_details(
    state: State<'_, AppState>,
    id: String,
    refresh: bool,
) -> Result<ServerDetails, String> {
    let (ip, query_port) = id
        .rsplit_once(':')
        .and_then(|(ip, port)| Some((ip.to_string(), port.parse::<u16>().ok()?)))
        .ok_or_else(|| format!("`{id}` is not a valid ip:port address"))?;

    let cached = state.store.read().await.find(&id).cloned();

    let mut warning = None;
    let mut live = false;

    let mut server = match cached {
        Some(server) => server,
        // Not in the list: a live lookup is the only way to get anything.
        None => {
            live = true;
            dzsa::query_server(&ip, query_port).await.map_err(|e| {
                format!("{id} is not in the server list and did not answer a lookup: {e}")
            })?
        }
    };

    if refresh && !live {
        match dzsa::query_server(&ip, query_port).await {
            Ok(fresh) => {
                server = fresh;
                live = true;
            }
            Err(e) => warning = Some(format!("Live lookup failed ({e}). Showing cached data.")),
        }
    }

    let env = current_env(&state).await;
    let workshop = env.workshop_dir.clone();

    let mods: Vec<ModStatus> = server
        .mods
        .iter()
        .map(|m| ModStatus {
            id: m.id,
            name: if m.name.trim().is_empty() {
                format!("Mod {}", m.id)
            } else {
                m.name.clone()
            },
            installed: workshop
                .as_ref()
                .map(|dir| Path::new(dir).join(m.id.to_string()).is_dir())
                .unwrap_or(false),
        })
        .collect();

    Ok(ServerDetails {
        connect: server.connect_string(),
        missing_count: mods.iter().filter(|m| !m.installed).count(),
        server: ServerRow::from(&server),
        mods,
        live,
        warning,
    })
}

#[tauri::command]
async fn ping_servers(
    ids: Vec<String>,
    timeout_ms: Option<u64>,
) -> Result<Vec<a2s::PingResult>, String> {
    let targets: Vec<(String, u16)> = ids
        .iter()
        .filter_map(|id| {
            let (ip, port) = id.rsplit_once(':')?;
            Some((ip.to_string(), port.parse().ok()?))
        })
        .collect();

    Ok(a2s::ping_many(targets, timeout_ms.unwrap_or(1_200)).await)
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

#[tauri::command]
async fn installed_mods(
    state: State<'_, AppState>,
    with_sizes: bool,
) -> Result<Vec<InstalledMod>, String> {
    let env = current_env(&state).await;
    let workshop = env.workshop_dir.clone();

    tokio::task::spawn_blocking(move || {
        steam::installed_mods(workshop.as_deref().map(Path::new), with_sizes)
    })
    .await
    .map_err(|e| format!("Could not read the mod folder: {e}"))
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModActionResult {
    /// How many items Steam accepted.
    count: usize,
    /// True when we had to fall back to a plain download because the Workshop
    /// helper could not run — the mods arrive, but unsubscribed.
    downloaded_only: bool,
    /// Why the fallback kicked in, so the UI can be honest about it.
    warning: Option<String>,
}

/// Subscribes to every missing mod. Subscribing (rather than just downloading)
/// is what makes Steam keep the mods updated and lets the user drop them later.
///
/// If the helper cannot run — Steam closed, `libsteam_api.so` missing — this
/// falls back to a plain download so the user can still play, and says so.
#[tauri::command]
async fn subscribe_mods(
    state: State<'_, AppState>,
    ids: Vec<u64>,
) -> Result<ModActionResult, String> {
    let env = current_env(&state).await;
    let total = ids.len();

    let outcome = tokio::task::spawn_blocking({
        let env = env.clone();
        let ids = ids.clone();
        move || steam::workshop_action(&env, true, &ids)
    })
    .await
    .map_err(|e| format!("Could not subscribe: {e}"))?;

    match outcome {
        Ok(result) => Ok(ModActionResult {
            count: result.ok.len(),
            downloaded_only: false,
            warning: (!result.failed.is_empty() || !result.timed_out.is_empty()).then(|| {
                format!(
                    "{} of {total} item(s) did not go through. Try again in a moment.",
                    result.failed.len() + result.timed_out.len()
                )
            }),
        }),
        Err(reason) => {
            // Subscribing failed; at least get the files down so the user can
            // play, and be explicit that Steam will not keep them updated.
            tokio::task::spawn_blocking(move || steam::download_workshop_items(&env, &ids))
                .await
                .map_err(|e| format!("Could not start the download: {e}"))??;

            Ok(ModActionResult {
                count: total,
                downloaded_only: true,
                warning: Some(format!(
                    "Could not subscribe ({reason}). Downloading instead — the mods will work, \
                     but Steam will not auto-update them."
                )),
            })
        }
    }
}

/// Subscription state for the given mods, so the UI can flag the ones Steam
/// does not track and offer to delete them instead.
#[tauri::command]
async fn mod_states(
    state: State<'_, AppState>,
    ids: Vec<u64>,
) -> Result<Vec<steam::ItemState>, String> {
    let env = current_env(&state).await;

    tokio::task::spawn_blocking(move || steam::workshop_states(&env, &ids))
        .await
        .map_err(|e| format!("Could not query Steam: {e}"))?
}

/// Deletes mods Steam does not track. Unsubscribing cannot remove these,
/// because there is no subscription to drop.
#[tauri::command]
async fn delete_mods(state: State<'_, AppState>, ids: Vec<u64>) -> Result<usize, String> {
    let env = current_env(&state).await;
    let workshop = env.workshop_dir.clone();

    tokio::task::spawn_blocking(move || {
        steam::delete_mod_folders(workshop.as_deref().map(Path::new), &ids)
    })
    .await
    .map_err(|e| format!("Could not delete the mods: {e}"))?
}

/// Drops the subscription for these mods. Steam removes the files itself.
#[tauri::command]
async fn unsubscribe_mods(state: State<'_, AppState>, ids: Vec<u64>) -> Result<usize, String> {
    let env = current_env(&state).await;

    let outcome = tokio::task::spawn_blocking(move || steam::workshop_action(&env, false, &ids))
        .await
        .map_err(|e| format!("Could not unsubscribe: {e}"))??;

    Ok(outcome.ok.len())
}

/// Which of these mods are on disk right now. Polled while a download runs.
#[tauri::command]
async fn mods_installed(state: State<'_, AppState>, ids: Vec<u64>) -> Result<Vec<u64>, String> {
    let env = current_env(&state).await;
    let workshop = env.workshop_dir.clone();

    tokio::task::spawn_blocking(move || {
        steam::installed_ids(workshop.as_deref().map(Path::new), &ids)
    })
    .await
    .map_err(|e| format!("Could not read the mod folder: {e}"))
}

#[tauri::command]
async fn prune_symlinks(state: State<'_, AppState>) -> Result<usize, String> {
    let env = current_env(&state).await;
    let dayz_dir = env
        .dayz_dir
        .clone()
        .ok_or("No DayZ installation detected")?;

    tokio::task::spawn_blocking(move || steam::prune_broken_symlinks(Path::new(&dayz_dir)))
        .await
        .map_err(|e| format!("Could not clean up the links: {e}"))
}

// ---------------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------------

#[tauri::command]
async fn launch_game(
    state: State<'_, AppState>,
    request: LaunchRequest,
) -> Result<LaunchOutcome, String> {
    let env = current_env(&state).await;
    tokio::task::spawn_blocking(move || steam::launch(&env, &request))
        .await
        .map_err(|e| format!("Could not launch the game: {e}"))?
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    steam::open_url(&url)
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.lock().await.clone())
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    let steam_root_changed = {
        let current = state.config.lock().await;
        current.custom_steam_root != config.custom_steam_root
    };

    config::save(&config)?;
    *state.config.lock().await = config.clone();

    // Changing the Steam path means the detection has to run again.
    if steam_root_changed {
        refresh_env(&state).await;
    }

    Ok(config)
}

#[tauri::command]
async fn toggle_favorite(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    let mut config = state.config.lock().await;
    config.toggle_favorite(&id);
    config::save(&config)?;
    Ok(config.clone())
}

#[tauri::command]
async fn record_launch(
    state: State<'_, AppState>,
    id: String,
    name: String,
    map: String,
) -> Result<AppConfig, String> {
    let mut config = state.config.lock().await;
    config.record_launch(&id, &name, &map);
    config::save(&config)?;
    Ok(config.clone())
}

fn main() {
    // WebKitGTK's DMABUF renderer leaves a blank window — or dies with
    // "Gdk-Message: Error 71 dispatching to Wayland display" — on a lot of
    // Wayland sessions. Forcing the GL backend before any webview code loads
    // the library makes the app boot from a terminal, a file manager or Steam
    // alike, in every bundle, with nothing for the user to export. A value the
    // user set themselves always wins.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState {
                store: RwLock::new(ServerStore::default()),
                env: RwLock::new(None),
                config: Mutex::new(config::load()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            steam_environment,
            refresh_servers,
            query_servers,
            server_maps,
            server_details,
            ping_servers,
            installed_mods,
            subscribe_mods,
            unsubscribe_mods,
            mod_states,
            delete_mods,
            mods_installed,
            prune_symlinks,
            launch_game,
            open_url,
            get_config,
            save_config,
            toggle_favorite,
            record_launch,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Balota");
}
