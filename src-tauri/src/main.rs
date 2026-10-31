// SPDX-License-Identifier: GPL-3.0-or-later
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

use config::{AppConfig, SettingsPatch};
use servers::{ListStatus, MapCount, ServerFilter, ServerPage, ServerRow, ServerStore};
use steam::{InstalledMod, LaunchOutcome, LaunchRequest, SteamEnvironment};

struct AppState {
    store: RwLock<ServerStore>,
    env: RwLock<Option<SteamEnvironment>>,
    config: Mutex<AppConfig>,
    /// Held for the length of a list refresh. Holding F5 used to start a 26 MB
    /// download per repeat.
    refresh: Mutex<()>,
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
    let env = tokio::task::spawn_blocking({
        let custom_root = custom_root.clone();
        move || steam::detect(custom_root.as_deref())
    })
    .await
    // A panic in detection is not a reason to lose the custom path the user
    // set; retry with it rather than without.
    .unwrap_or_else(|_| steam::detect(custom_root.as_deref()));

    *state.env.write().await = Some(env.clone());
    env
}

/// Every Workshop folder that exists right now.
///
/// Deliberately recomputed instead of read from the cached environment: the
/// folder does not exist until Steam finishes the first download, and a value
/// captured at startup left the launcher insisting that mods already on disk
/// still needed downloading.
async fn workshop_dirs(state: &AppState) -> Vec<std::path::PathBuf> {
    steam::workshop_dirs_now(&current_env(state).await)
}

#[tauri::command]
async fn steam_environment(
    state: State<'_, AppState>,
    refresh: bool,
) -> Result<SteamEnvironment, String> {
    let mut env = if refresh {
        refresh_env(&state).await
    } else {
        current_env(&state).await
    };

    // Cheap, and it keeps the Settings tab honest between subscriptions.
    env.workshop_dirs = steam::workshop_dirs_now(&env)
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    Ok(env)
}

// ---------------------------------------------------------------------------
// Server list
// ---------------------------------------------------------------------------

/// Reads and parses the disk cache off the async runtime: doing 26 MB of it
/// there, with the store's write lock held, blocked every other query.
async fn load_cache(fresh_only: bool) -> Result<Option<(Vec<dzsa::ApiServer>, u64)>, String> {
    tokio::task::spawn_blocking(move || {
        servers::read_cache(fresh_only)
            .and_then(|(body, modified)| servers::parse(&body).ok().map(|list| (list, modified)))
    })
    .await
    .map_err(|e| format!("Could not read the cached list: {e}"))
}

#[tauri::command]
async fn refresh_servers(state: State<'_, AppState>, force: bool) -> Result<ListStatus, String> {
    // A refresh already running is not an error: holding F5 should be a no-op,
    // not a queue of 26 MB downloads.
    let Ok(_guard) = state.refresh.try_lock() else {
        return Ok(state.store.read().await.status());
    };

    if !force {
        if let Some((list, modified)) = load_cache(true).await? {
            return Ok(state.store.write().await.adopt(list, modified, true));
        }
    }

    let (list, body) = match dzsa::fetch_master_list().await {
        Ok(fetched) => fetched,
        // The API being down says nothing about the servers, which are almost
        // always still up. Keep whatever list there is instead of replacing
        // it with an error screen: the one already in memory if there is
        // one, otherwise the disk cache however old it is.
        Err(e) => {
            let note = format!("{e} Showing the last list that could be downloaded.");
            if !state.store.read().await.is_empty() {
                let mut status = state.store.read().await.status();
                status.warning = Some(note);
                return Ok(status);
            }
            if let Some((list, modified)) = load_cache(false).await? {
                let mut status = state.store.write().await.adopt(list, modified, true);
                status.warning = Some(note);
                return Ok(status);
            }
            return Err(e);
        }
    };
    let count = list.len();
    let status = state
        .store
        .write()
        .await
        .adopt(list, servers::now_secs(), false);

    // An empty list is what the API returns while it is having a bad day.
    // Writing that over a good cache buys 15 minutes of "no servers found".
    if count > 0 {
        tokio::task::spawn_blocking(move || servers::write_cache(&body));
    }

    Ok(status)
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

/// Splits an `ip:query_port` server id, refusing anything that is not one.
fn parse_server_id(id: &str) -> Result<(String, u16), String> {
    let (ip, query_port) = id
        .rsplit_once(':')
        .and_then(|(ip, port)| Some((ip.to_string(), port.parse::<u16>().ok()?)))
        .ok_or_else(|| format!("`{id}` is not a valid ip:port address"))?;
    ip.parse::<std::net::IpAddr>()
        .map_err(|_| format!("`{ip}` is not a valid IP address"))?;
    Ok((ip, query_port))
}

/// More mod ids than any server has ever needed. Every list of them that comes
/// from the interface is held to it: they end up on a command line, and an
/// unbounded one fails to spawn at all.
const MAX_IDS: usize = 500;

fn check_ids(ids: &[u64]) -> Result<(), String> {
    if ids.len() > MAX_IDS {
        return Err(format!(
            "Too many mods at once ({}, at most {MAX_IDS}).",
            ids.len()
        ));
    }
    Ok(())
}

/// Favourites are kept in the config file, which is rewritten on every star.
const MAX_FAVORITES: usize = 1_000;

/// How long dayzsalauncher.com gets to answer once the server itself already
/// has. Its mod names are the Workshop titles, which read better than the
/// internal ones a server reports, but not at the price of a long wait.
const API_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Looks a server up through dayzsalauncher.com and, at the same time, by
/// asking the server directly over A2S. The API is preferred; the direct
/// answer is what makes joining possible while the API is down, which it was
/// for a whole day once while every server stayed up.
///
/// Returns the server and, when the direct answer was used, a note saying so.
async fn live_lookup(
    ip: &str,
    query_port: u16,
) -> Result<(dzsa::ApiServer, Option<String>), String> {
    let api = dzsa::query_server(ip, query_port);
    let direct = a2s::lookup(ip, query_port);
    tokio::pin!(api, direct);

    let note = |truncated: bool| {
        let mut note = "dayzsalauncher.com did not answer, so these details come straight from \
                        the server."
            .to_string();
        if truncated {
            note.push_str(
                " It has more mods than it can announce, so some are missing from this list.",
            );
        }
        Some(note)
    };

    tokio::select! {
        result = &mut api => match result {
            Ok(server) => Ok((server, None)),
            Err(api_error) => match direct.await {
                Ok((server, truncated)) => Ok((server, note(truncated))),
                Err(direct_error) => Err(format!("{api_error}; asking the server directly: {direct_error}")),
            },
        },
        result = &mut direct => match result {
            Ok((server, truncated)) => match tokio::time::timeout(API_GRACE, &mut api).await {
                Ok(Ok(from_api)) => Ok((from_api, None)),
                _ => Ok((server, note(truncated))),
            },
            Err(direct_error) => api
                .await
                .map(|server| (server, None))
                .map_err(|api_error| format!("{api_error}; asking the server directly: {direct_error}")),
        },
    }
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
    // The address reaches a URL path and, later, the game's command line.
    let (ip, query_port) = parse_server_id(&id)?;

    let cached = state.store.read().await.find(&id).cloned();

    let mut warning = None;
    let mut live = false;

    let mut server = match cached {
        Some(server) => server,
        // Not in the list: a live lookup is the only way to get anything.
        None => {
            live = true;
            let (server, note) = live_lookup(&ip, query_port).await.map_err(|e| {
                format!("{id} is not in the server list and did not answer a lookup: {e}")
            })?;
            warning = note;
            server
        }
    };

    if refresh && !live {
        match live_lookup(&ip, query_port).await {
            Ok((fresh, note)) => {
                server = fresh;
                live = true;
                warning = note;
            }
            Err(e) => warning = Some(format!("Live lookup failed ({e}). Showing cached data.")),
        }
    }

    let dirs = workshop_dirs(&state).await;
    let ids: Vec<u64> = server.mods.iter().map(|m| m.id).collect();
    let installed = tokio::task::spawn_blocking(move || steam::installed_ids(&dirs, &ids))
        .await
        .unwrap_or_default();

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
            installed: installed.contains(&m.id),
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
        .take(a2s::MAX_TARGETS)
        .filter_map(|id| {
            let (ip, port) = id.rsplit_once(':')?;
            // Only real addresses get probed: the list is third-party data and
            // this is what turns it into UDP traffic.
            ip.parse::<std::net::IpAddr>().ok()?;
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
    let dirs = workshop_dirs(&state).await;

    tokio::task::spawn_blocking(move || steam::installed_mods(&dirs, with_sizes))
        .await
        .map_err(|e| format!("Could not read the mod folder: {e}"))
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModActionResult {
    /// How many items Steam accepted.
    count: usize,
    /// The IDs Steam actually took, so the interface can wait for those rather
    /// than for the ones it asked about.
    accepted: Vec<u64>,
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
    check_ids(&ids)?;
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
        Ok(result) => {
            let rejected = result.failed.len() + result.timed_out.len();
            Ok(ModActionResult {
                count: result.ok.len(),
                warning: (rejected > 0).then(|| {
                    format!(
                        "{rejected} of {total} item(s) did not go through. They may have been \
                         removed from the Workshop, or Steam may be busy — try again in a moment."
                    )
                }),
                accepted: result.ok,
                downloaded_only: false,
            })
        }
        Err(reason) => {
            // Subscribing failed; at least get the files down so the user can
            // play, and be explicit that Steam will not keep them updated.
            tokio::task::spawn_blocking({
                let env = env.clone();
                let ids = ids.clone();
                move || steam::download_workshop_items(&env, &ids)
            })
            .await
            .map_err(|e| format!("Could not start the download: {e}"))??;

            Ok(ModActionResult {
                count: total,
                accepted: ids,
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
    check_ids(&ids)?;
    let env = current_env(&state).await;

    tokio::task::spawn_blocking(move || steam::workshop_states(&env, &ids))
        .await
        .map_err(|e| format!("Could not query Steam: {e}"))?
}

/// How big these mods are, so the user can see what they are agreeing to
/// before a multi-gigabyte download starts.
#[tauri::command]
async fn mod_sizes(
    state: State<'_, AppState>,
    ids: Vec<u64>,
) -> Result<steam::ModInfoReply, String> {
    check_ids(&ids)?;
    let env = current_env(&state).await;

    tokio::task::spawn_blocking(move || steam::workshop_info(&env, &ids))
        .await
        .map_err(|e| format!("Could not ask Steam about these mods: {e}"))?
}

/// Deletes mods Steam does not track. Unsubscribing cannot remove these,
/// because there is no subscription to drop.
#[tauri::command]
async fn delete_mods(state: State<'_, AppState>, ids: Vec<u64>) -> Result<usize, String> {
    check_ids(&ids)?;
    let dirs = workshop_dirs(&state).await;

    tokio::task::spawn_blocking(move || steam::delete_mod_folders(&dirs, &ids))
        .await
        .map_err(|e| format!("Could not delete the mods: {e}"))?
}

/// Drops the subscription for these mods. Steam removes the files itself.
#[tauri::command]
async fn unsubscribe_mods(state: State<'_, AppState>, ids: Vec<u64>) -> Result<usize, String> {
    check_ids(&ids)?;
    let env = current_env(&state).await;

    let outcome = tokio::task::spawn_blocking(move || steam::workshop_action(&env, false, &ids))
        .await
        .map_err(|e| format!("Could not unsubscribe: {e}"))??;

    Ok(outcome.ok.len())
}

/// Which of these mods are on disk right now. Polled while a download runs.
#[tauri::command]
async fn mods_installed(state: State<'_, AppState>, ids: Vec<u64>) -> Result<Vec<u64>, String> {
    check_ids(&ids)?;
    let dirs = workshop_dirs(&state).await;

    tokio::task::spawn_blocking(move || steam::installed_ids(&dirs, &ids))
        .await
        .map_err(|e| format!("Could not read the mod folder: {e}"))
}

#[tauri::command]
async fn prune_symlinks(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let env = current_env(&state).await;
    let dayz_dir = env
        .dayz_dir
        .clone()
        .ok_or("No DayZ installation detected")?;
    let dirs = steam::workshop_dirs_now(&env);

    tokio::task::spawn_blocking(move || steam::prune_broken_symlinks(Path::new(&dayz_dir), &dirs))
        .await
        .map_err(|e| format!("Could not clean up the links: {e}"))
}

// ---------------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------------

#[tauri::command]
async fn launch_game(
    state: State<'_, AppState>,
    mut request: LaunchRequest,
) -> Result<LaunchOutcome, String> {
    {
        let config = state.config.lock().await;
        request.player_name = Some(config.player_name.clone());
        request.extra_args = Some(config.extra_launch_args.clone());
    }
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

/// Applies the fields the Settings tab owns.
///
/// Takes a patch rather than a whole `AppConfig`: the interface used to send
/// back its own copy, so a keystroke in "in-game name" carried a stale
/// `favorites` list with it and undid a star clicked a moment earlier. Saving
/// happens with the lock still held, so two saves cannot interleave.
#[tauri::command]
async fn save_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<AppConfig, String> {
    let steam_root_changed;
    let updated;
    {
        let mut current = state.config.lock().await;
        let before = current.custom_steam_root.clone();
        current.apply(patch);
        steam_root_changed = before != current.custom_steam_root;

        config::save(&current)?;
        updated = current.clone();
    }

    // Changing the Steam path means the detection has to run again.
    if steam_root_changed {
        refresh_env(&state).await;
    }

    Ok(updated)
}

#[tauri::command]
async fn toggle_favorite(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    let mut config = state.config.lock().await;
    // Removing is always allowed, so a bad entry from an older version can go.
    if !config.favorites.contains(&id) {
        parse_server_id(&id)?;
        if config.favorites.len() >= MAX_FAVORITES {
            return Err(format!("At most {MAX_FAVORITES} favourites."));
        }
    }
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
    parse_server_id(&id)?;
    // Server names are third-party text; the history only needs to show them.
    let name: String = name.chars().take(256).collect();
    let map: String = map.chars().take(64).collect();
    let mut config = state.config.lock().await;
    config.record_launch(&id, &name, &map);
    config::save(&config)?;
    Ok(config.clone())
}

/// Drops one history entry. A server that no longer exists otherwise sits
/// there forever, opening a panel that fails and closes itself.
#[tauri::command]
async fn forget_launch(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    let mut config = state.config.lock().await;
    config.forget(&id);
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
    //
    // Safe here because nothing else has been started yet: this is the first
    // statement of `main`, long before any thread exists to race with.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let result = tauri::Builder::default()
        .setup(|app| {
            let mut config = config::load();

            // First run: offer Steam's own display name rather than DayZ's
            // placeholder, which a good number of servers kick on sight.
            //
            // A name already saved gets the same treatment when it is one of
            // those placeholders: earlier versions shipped "Survivor" as the
            // default, so an existing config carries the very string that gets
            // its owner kicked, and they have no way of knowing why.
            if steam::name_is_replaceable(&config.player_name) {
                let detected = steam::detect(config.custom_steam_root.as_deref());
                if let Some(name) = detected.suggested_name.clone() {
                    config.player_name = name;
                    let _ = config::save(&config);
                }
                app.manage(AppState {
                    store: RwLock::new(ServerStore::default()),
                    env: RwLock::new(Some(detected)),
                    config: Mutex::new(config),
                    refresh: Mutex::new(()),
                });
            } else {
                app.manage(AppState {
                    store: RwLock::new(ServerStore::default()),
                    env: RwLock::new(None),
                    config: Mutex::new(config),
                    refresh: Mutex::new(()),
                });
            }
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
            mod_sizes,
            delete_mods,
            mods_installed,
            prune_symlinks,
            launch_game,
            open_url,
            get_config,
            save_settings,
            toggle_favorite,
            record_launch,
            forget_launch,
        ])
        .run(tauri::generate_context!());

    // `expect` here printed nothing useful in a release build: the binary is
    // stripped, and a windowless failure looks exactly like the app not
    // starting at all. Leave something behind that can be pasted into a bug
    // report.
    if let Err(e) = result {
        let message = format!("Balota could not start: {e}");
        eprintln!("{message}");
        if let Some(dir) = dirs::cache_dir() {
            let dir = dir.join("balota");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("last-error.log"), &message);
        }
        std::process::exit(1);
    }
}
