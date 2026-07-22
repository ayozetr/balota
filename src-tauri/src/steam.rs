//! Steam installation discovery, mod symlink management and game launching.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;

use crate::vdf;

pub const DAYZ_APP_ID: &str = "221100";
pub const FLATPAK_STEAM_ID: &str = "com.valvesoftware.Steam";

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SteamEnvironment {
    pub steam_found: bool,
    pub is_flatpak: bool,
    pub steam_running: bool,
    pub steam_root: Option<String>,
    pub libraries: Vec<String>,
    pub dayz_dir: Option<String>,
    pub workshop_dir: Option<String>,
    pub dayz_found: bool,
    /// Diagnostics shown in the Settings tab when something is missing.
    /// Without them, "DayZ not found" never tells the user where we looked.
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModPreparation {
    /// Link names (`@xxxx`) ready to be passed to `-mod=`.
    pub linked: Vec<String>,
    /// IDs the server requires that are not downloaded yet.
    pub missing: Vec<u64>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub command: String,
    pub linked_mods: Vec<String>,
    pub missing_mods: Vec<u64>,
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Places a Steam installation can live, in order of preference. The boolean
/// marks the Flatpak build.
fn candidate_roots(custom_root: Option<&str>) -> Vec<(PathBuf, bool)> {
    let mut out: Vec<(PathBuf, bool)> = Vec::new();

    if let Some(custom) = custom_root {
        let custom = custom.trim();
        if !custom.is_empty() {
            out.push((
                PathBuf::from(custom),
                custom.contains(".var/app/com.valvesoftware.Steam"),
            ));
        }
    }

    if let Some(home) = dirs::home_dir() {
        out.push((home.join(".local/share/Steam"), false));
        out.push((home.join(".steam/steam"), false));
        out.push((home.join(".steam/root"), false));
        out.push((
            home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
            true,
        ));
        out.push((home.join("snap/steam/common/.local/share/Steam"), false));
    }

    // Several of these paths are symlinks to each other; canonicalize so the
    // same installation is not processed twice.
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for (path, flatpak) in out {
        if !path.join("steamapps").is_dir() {
            continue;
        }
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key.clone()) {
            deduped.push((key, flatpak));
        }
    }

    deduped
}

/// Every library declared in `libraryfolders.vdf`, root included. This is what
/// finds DayZ when it lives on a second drive or on the Steam Deck's SD card.
fn libraries_of(root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![root.to_path_buf()];

    if let Ok(content) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        if let Some(folders) = vdf::parse(&content).get("libraryfolders") {
            for (_, entry) in folders.entries() {
                if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                    let path = PathBuf::from(path);
                    if path.join("steamapps").is_dir() {
                        libs.push(path);
                    }
                }
            }
        }
    }

    let mut seen = BTreeSet::new();
    libs.retain(|p| {
        let key = fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        seen.insert(key)
    });

    libs
}

/// DayZ's folder inside a library. Read from the app manifest because
/// `installdir` is not necessarily the literal string "DayZ".
fn dayz_in_library(library: &Path) -> Option<PathBuf> {
    let steamapps = library.join("steamapps");

    if let Ok(content) =
        fs::read_to_string(steamapps.join(format!("appmanifest_{DAYZ_APP_ID}.acf")))
    {
        if let Some(dir) = vdf::parse(&content)
            .get("AppState")
            .and_then(|v| v.get("installdir"))
            .and_then(|v| v.as_str())
        {
            let candidate = steamapps.join("common").join(dir);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    let fallback = steamapps.join("common/DayZ");
    fallback.is_dir().then_some(fallback)
}

fn workshop_in_library(library: &Path) -> Option<PathBuf> {
    let path = library.join("steamapps/workshop/content").join(DAYZ_APP_ID);
    path.is_dir().then_some(path)
}

/// One Steam installation and what was found inside it.
struct Candidate {
    root: PathBuf,
    is_flatpak: bool,
    libraries: Vec<PathBuf>,
    dayz: Option<PathBuf>,
    workshop: Option<PathBuf>,
}

fn process_running(name: &str) -> bool {
    Command::new("pgrep")
        .args(["-x", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn flatpak_steam_running() -> bool {
    Command::new("flatpak")
        .arg("ps")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).contains(FLATPAK_STEAM_ID))
        .unwrap_or(false)
}

pub fn detect(custom_root: Option<&str>) -> SteamEnvironment {
    let mut notes = Vec::new();
    let roots = candidate_roots(custom_root);

    if roots.is_empty() {
        notes.push(
            "No Steam installation found in the usual places (~/.local/share/Steam, \
             ~/.steam/steam, Flatpak, Snap). If yours lives somewhere else, set the path \
             manually below."
                .to_string(),
        );
        return SteamEnvironment {
            steam_found: false,
            is_flatpak: false,
            steam_running: false,
            steam_root: None,
            libraries: Vec::new(),
            dayz_dir: None,
            workshop_dir: None,
            dayz_found: false,
            notes,
        };
    }

    // Prefer the first installation that actually has DayZ; otherwise fall
    // back to the first one that exists.
    let mut chosen: Option<Candidate> = None;

    for (root, is_flatpak) in &roots {
        let libraries = libraries_of(root);
        let dayz = libraries.iter().find_map(|lib| dayz_in_library(lib));
        // The workshop folder usually sits in the same library as the game,
        // but not always: look there first, then everywhere else.
        let workshop = dayz
            .as_ref()
            .and_then(|d| {
                // .../<lib>/steamapps/common/DayZ -> .../<lib>
                let lib = d.parent()?.parent()?.parent()?;
                workshop_in_library(lib)
            })
            .or_else(|| libraries.iter().find_map(|lib| workshop_in_library(lib)));

        let has_dayz = dayz.is_some();
        if chosen.is_none() || has_dayz {
            chosen = Some(Candidate {
                root: root.clone(),
                is_flatpak: *is_flatpak,
                libraries,
                dayz,
                workshop,
            });
        }
        if has_dayz {
            break;
        }
    }

    let Candidate {
        root,
        is_flatpak,
        libraries: libs,
        dayz,
        workshop,
    } = chosen.expect("the root list is not empty");

    if dayz.is_none() {
        notes.push(format!(
            "Steam found at {}, but DayZ is not installed in any of its {} librar(ies). \
             Install it from Steam, or check that the library is mounted.",
            root.display(),
            libs.len()
        ));
    }
    if dayz.is_some() && workshop.is_none() {
        notes.push(
            "DayZ is installed but the Workshop mod folder does not exist yet. It shows up as \
             soon as you subscribe to your first mod."
                .to_string(),
        );
    }

    let steam_running = if is_flatpak {
        flatpak_steam_running()
    } else {
        process_running("steam")
    };
    if !steam_running {
        notes.push(if is_flatpak {
            "The Steam Flatpak is not running. Start it before launching the game: \
             `flatpak run com.valvesoftware.Steam`."
                .to_string()
        } else {
            "Steam is not running. Balota will start it when you launch the game, but the first \
             launch will take longer than usual."
                .to_string()
        });
    }

    SteamEnvironment {
        steam_found: true,
        is_flatpak,
        steam_running,
        steam_root: Some(root.to_string_lossy().to_string()),
        libraries: libs
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        dayz_found: dayz.is_some(),
        dayz_dir: dayz.map(|p| p.to_string_lossy().to_string()),
        workshop_dir: workshop.map(|p| p.to_string_lossy().to_string()),
        notes,
    }
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

/// Link name for a Workshop mod.
///
/// DayZ under Proton chokes on mod names with spaces or non-ASCII characters,
/// and the `-mod=` command line has a length limit that a server with 40 mods
/// blows through easily. The fix is to encode the ID as a little-endian
/// integer in base64, swapping `/` for `-` and `+` for `_`. Note this is not
/// standard URL-safe base64, which maps those two characters the other way
/// round.
pub fn mod_symlink_name(mod_id: u64) -> String {
    let mut bytes = Vec::new();
    let mut value = mod_id;
    loop {
        bytes.push((value & 0xff) as u8);
        value >>= 8;
        if value == 0 {
            break;
        }
    }

    let encoded = STANDARD_NO_PAD
        .encode(&bytes)
        .replace('/', "-")
        .replace('+', "_");

    format!("@{encoded}")
}

/// Creates (or repairs) the mod symlinks inside DayZ's folder and returns the
/// names ready for `-mod=`.
pub fn prepare_mod_symlinks(
    dayz_dir: &Path,
    workshop_dir: Option<&Path>,
    mod_ids: &[u64],
) -> ModPreparation {
    let mut prep = ModPreparation {
        linked: Vec::new(),
        missing: Vec::new(),
        errors: Vec::new(),
    };

    if mod_ids.is_empty() {
        return prep;
    }

    let Some(workshop_dir) = workshop_dir else {
        prep.missing.extend_from_slice(mod_ids);
        return prep;
    };

    for &mod_id in mod_ids {
        let source = workshop_dir.join(mod_id.to_string());
        if !source.is_dir() {
            prep.missing.push(mod_id);
            continue;
        }

        let link_name = mod_symlink_name(mod_id);
        let link_path = dayz_dir.join(&link_name);

        // `symlink_metadata` does not follow the link, so it also catches
        // dangling links — which `exists()` reports as missing, making the
        // subsequent create fail with EEXIST.
        match fs::symlink_metadata(&link_path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target_ok = fs::read_link(&link_path)
                    .map(|t| t == source)
                    .unwrap_or(false);
                if target_ok {
                    prep.linked.push(link_name);
                    continue;
                }
                if let Err(e) = fs::remove_file(&link_path) {
                    prep.errors
                        .push(format!("Could not recreate link {link_name}: {e}"));
                    continue;
                }
            }
            Ok(_) => {
                // Something that is not a link is already there (a real mod
                // folder). Leave it alone: it works as-is.
                prep.linked.push(link_name);
                continue;
            }
            Err(_) => {}
        }

        match std::os::unix::fs::symlink(&source, &link_path) {
            Ok(()) => prep.linked.push(link_name),
            Err(e) => prep
                .errors
                .push(format!("Could not link mod {mod_id}: {e}")),
        }
    }

    prep
}

/// Removes `@...` links in DayZ's folder that no longer point at a downloaded
/// mod.
pub fn prune_broken_symlinks(dayz_dir: &Path) -> usize {
    let mut removed = 0;
    let Ok(entries) = fs::read_dir(dayz_dir) else {
        return 0;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_mod_link = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('@'))
            .unwrap_or(false);
        if !is_mod_link {
            continue;
        }

        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        // `path.exists()` follows the link: false means the target is gone.
        if meta.file_type().is_symlink() && !path.exists() && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }

    removed
}

fn mod_name_from_meta(meta_path: &Path) -> Option<String> {
    let content = fs::read_to_string(meta_path).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("name") {
            continue;
        }
        let start = line.find('"')?;
        let rest = &line[start + 1..];
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    None
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn installed_mods(workshop_dir: Option<&Path>, with_sizes: bool) -> Vec<InstalledMod> {
    let Some(workshop_dir) = workshop_dir else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(workshop_dir) else {
        return Vec::new();
    };

    let mut mods: Vec<InstalledMod> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let id: u64 = path.file_name()?.to_str()?.parse().ok()?;
            let name =
                mod_name_from_meta(&path.join("meta.cpp")).unwrap_or_else(|| format!("Mod {id}"));

            Some(InstalledMod {
                id,
                name,
                path: path.to_string_lossy().to_string(),
                size_bytes: if with_sizes { dir_size(&path) } else { 0 },
            })
        })
        .collect();

    mods.sort_by_key(|m| m.name.to_lowercase());
    mods
}

// ---------------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    /// `ip:game_port` — the game port, not the query port.
    pub connect: Option<String>,
    #[serde(default)]
    pub mod_ids: Vec<u64>,
    pub player_name: Option<String>,
    pub extra_args: Option<String>,
    /// When `true`, returns the command without running it.
    #[serde(default)]
    pub dry_run: bool,
}

pub fn launch(env: &SteamEnvironment, req: &LaunchRequest) -> Result<LaunchOutcome, String> {
    let dayz_dir = env
        .dayz_dir
        .as_ref()
        .ok_or("No DayZ installation detected")?;
    let dayz_dir = Path::new(dayz_dir);
    let workshop_dir = env.workshop_dir.as_ref().map(Path::new);

    let prep = prepare_mod_symlinks(dayz_dir, workshop_dir, &req.mod_ids);
    if !prep.errors.is_empty() {
        return Err(prep.errors.join("\n"));
    }
    if !prep.missing.is_empty() {
        return Err(format!(
            "{} mod(s) still need downloading. Install them from the server panel before joining.",
            prep.missing.len()
        ));
    }

    let mut args: Vec<String> = Vec::new();
    let program = if env.is_flatpak {
        args.extend(
            [
                "run",
                "--branch=stable",
                "--arch=x86_64",
                "--command=/app/bin/steam-wrapper",
                FLATPAK_STEAM_ID,
            ]
            .map(String::from),
        );
        "flatpak"
    } else {
        "steam"
    };

    args.push("-applaunch".into());
    args.push(DAYZ_APP_ID.into());

    if !prep.linked.is_empty() {
        args.push(format!("-mod={}", prep.linked.join(";")));
    }

    if let Some(connect) = req
        .connect
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push(format!("-connect={connect}"));
        args.push("-nolauncher".into());
        args.push("-world=empty".into());
    }

    if let Some(name) = req
        .player_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push(format!("-name={name}"));
    }

    if let Some(extra) = req.extra_args.as_deref() {
        args.extend(extra.split_whitespace().map(String::from));
    }

    let command = format!("{program} {}", args.join(" "));

    if !req.dry_run {
        Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not run `{program}`: {e}"))?;
    }

    Ok(LaunchOutcome {
        command,
        linked_mods: prep.linked,
        missing_mods: prep.missing,
    })
}

/// Asks the running Steam client to download Workshop items.
///
/// This is the client's own content pipeline: no web requests, so no rate
/// limiting, and it reuses the session Steam already has — the launcher never
/// sees a credential. Every ID goes in a single invocation, which Steam queues
/// and downloads in the background.
///
/// The alternative — opening one Workshop page per mod so the user can hit
/// Subscribe — does not survive contact with reality: a 40-mod server means 40
/// page loads, which Steam's web frontend rate-limits as abuse and answers with
/// a temporary block.
///
/// Caveat: downloaded items are not *subscribed* items, so Steam will not
/// auto-update them. Re-running this for an ID re-downloads it, which is how an
/// out-of-date mod gets refreshed.
pub fn download_workshop_items(env: &SteamEnvironment, mod_ids: &[u64]) -> Result<String, String> {
    if mod_ids.is_empty() {
        return Ok(String::new());
    }

    let mut args: Vec<String> = Vec::new();
    let program = if env.is_flatpak {
        args.extend(
            [
                "run",
                "--branch=stable",
                "--arch=x86_64",
                "--command=/app/bin/steam-wrapper",
                FLATPAK_STEAM_ID,
            ]
            .map(String::from),
        );
        "flatpak"
    } else {
        "steam"
    };

    for id in mod_ids {
        args.push("+workshop_download_item".into());
        args.push(DAYZ_APP_ID.into());
        args.push(id.to_string());
    }

    Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not ask Steam to download the mods: {e}"))?;

    Ok(format!("{program} {}", args.join(" ")))
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopOutcome {
    #[serde(default)]
    pub ok: Vec<u64>,
    #[serde(default)]
    pub failed: Vec<WorkshopFailure>,
    #[serde(default)]
    pub timed_out: Vec<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkshopFailure {
    pub id: u64,
    pub error: String,
}

/// Looks for the `balota-workshop` helper alongside a given executable
/// directory. Split out from `helper_path` so the lookup order is testable
/// without depending on where the test binary happens to live.
fn helper_in(dir: &Path) -> Option<PathBuf> {
    let candidates = [
        dir.join("balota-workshop"),
        // Tauri sidecars keep their target-triple suffix in some layouts.
        dir.join("balota-workshop-x86_64-unknown-linux-gnu"),
        dir.join("../lib/balota/balota-workshop"),
    ];

    candidates.into_iter().find(|path| path.is_file())
}

/// Locates the helper: next to the running executable when installed or
/// bundled, in the Cargo target directory during development.
fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    helper_in(exe.parent()?)
}

/// Where to find `libsteam_api.so`.
///
/// The user's own Steam install is preferred, so a `.deb` or a source build
/// needs no copy of Valve's library at all. Inside an AppImage the bundled copy
/// next to the helper is used instead, since the host paths may not exist.
fn steam_lib_dirs(env: &SteamEnvironment, helper: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(root) = &env.steam_root {
        let root = Path::new(root);
        dirs.push(root.join("steamrt64"));
        dirs.push(root.join("linux64"));
        dirs.push(root.join("ubuntu12_64"));
    }

    if let Some(helper_dir) = helper.parent() {
        dirs.push(helper_dir.to_path_buf());
        dirs.push(helper_dir.join("../lib"));
    }

    dirs.retain(|dir| dir.join("libsteam_api.so").is_file());
    dirs
}

/// Subscribes or unsubscribes Workshop items through the helper process.
///
/// Subscribing is what the user actually wants: Steam then keeps the mods
/// updated on its own and they can be removed from the Steam UI later, which
/// downloading alone does not give you.
pub fn workshop_action(
    env: &SteamEnvironment,
    subscribe: bool,
    mod_ids: &[u64],
) -> Result<WorkshopOutcome, String> {
    if mod_ids.is_empty() {
        return Ok(WorkshopOutcome::default());
    }

    let helper = helper_path().ok_or("The balota-workshop helper is missing")?;
    let lib_dirs = steam_lib_dirs(env, &helper);
    if lib_dirs.is_empty() {
        return Err("Could not find libsteam_api.so in the Steam installation".to_string());
    }

    let mut command = Command::new(helper);
    command
        .arg(if subscribe {
            "subscribe"
        } else {
            "unsubscribe"
        })
        .args(mod_ids.iter().map(u64::to_string))
        .env(
            "LD_LIBRARY_PATH",
            std::env::join_paths(&lib_dirs).map_err(|e| e.to_string())?,
        )
        .stdin(Stdio::null())
        .stderr(Stdio::null());

    let output = command
        .output()
        .map_err(|e| format!("Could not run the Workshop helper: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .last()
        .ok_or("The Workshop helper returned nothing")?;

    let outcome: WorkshopOutcome = serde_json::from_str(line)
        .map_err(|e| format!("Unreadable reply from the Workshop helper: {e}"))?;

    if let Some(error) = &outcome.error {
        return Err(error.clone());
    }

    Ok(outcome)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ItemState {
    pub id: u64,
    pub subscribed: bool,
    pub installed: bool,
}

#[derive(Debug, Deserialize)]
struct StateReply {
    #[serde(default)]
    items: Vec<ItemState>,
}

/// Asks Steam which of these items are actually subscribed.
///
/// A mod can sit in the Workshop folder without Steam knowing about it — that
/// is what a plain download leaves behind. Those never update, and
/// unsubscribing cannot remove them because there is no subscription to drop;
/// the only way out is deleting the folder.
pub fn workshop_states(env: &SteamEnvironment, mod_ids: &[u64]) -> Result<Vec<ItemState>, String> {
    if mod_ids.is_empty() {
        return Ok(Vec::new());
    }

    let helper = helper_path().ok_or("The balota-workshop helper is missing")?;
    let lib_dirs = steam_lib_dirs(env, &helper);
    if lib_dirs.is_empty() {
        return Err("Could not find libsteam_api.so in the Steam installation".to_string());
    }

    let output = Command::new(helper)
        .arg("state")
        .args(mod_ids.iter().map(u64::to_string))
        .env(
            "LD_LIBRARY_PATH",
            std::env::join_paths(&lib_dirs).map_err(|e| e.to_string())?,
        )
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("Could not query Steam: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().last().ok_or("Steam returned nothing")?;

    let reply: StateReply =
        serde_json::from_str(line).map_err(|e| format!("Unreadable reply from Steam: {e}"))?;

    Ok(reply.items)
}

/// Deletes downloaded mod folders outright. Only for items Steam does not
/// track: anything subscribed would simply be restored on the next sync.
pub fn delete_mod_folders(workshop_dir: Option<&Path>, mod_ids: &[u64]) -> Result<usize, String> {
    let workshop_dir = workshop_dir.ok_or("No Workshop folder detected")?;
    let mut removed = 0;

    for id in mod_ids {
        let path = workshop_dir.join(id.to_string());
        // Guard against a stray id deleting something outside the folder.
        if !path.starts_with(workshop_dir) || !path.is_dir() {
            continue;
        }
        fs::remove_dir_all(&path).map_err(|e| format!("Could not delete mod {id}: {e}"))?;
        removed += 1;
    }

    Ok(removed)
}

/// Which of these mods are already on disk.
pub fn installed_ids(workshop_dir: Option<&Path>, mod_ids: &[u64]) -> Vec<u64> {
    let Some(workshop_dir) = workshop_dir else {
        return Vec::new();
    };

    mod_ids
        .iter()
        .copied()
        .filter(|id| workshop_dir.join(id.to_string()).is_dir())
        .collect()
}

/// Opens a URL in the browser or in the Steam client. Only the schemes the app
/// actually needs are allowed through.
pub fn open_url(url: &str) -> Result<(), String> {
    const ALLOWED: [&str; 3] = ["https://", "http://", "steam://"];
    if !ALLOWED.iter().any(|prefix| url.starts_with(prefix)) {
        return Err(format!("URL scheme not allowed: {url}"));
    }

    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not open the URL: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-good output, pinned so a refactor cannot quietly change the
    /// naming: a wrong link name makes the game load no mods at all, and the
    /// only symptom is the server kicking you.
    #[test]
    fn mod_links_keep_their_exact_encoding() {
        assert_eq!(mod_symlink_name(1564026768), "@kCc5XQ");
        assert_eq!(mod_symlink_name(2940294109), "@3VNBrw");
        assert_eq!(mod_symlink_name(3322263460), "@pLcFxg");
        assert_eq!(mod_symlink_name(1), "@AQ");
        // Covers the '/' -> '-' substitution.
        assert_eq!(mod_symlink_name(255), "@-w");
        assert_eq!(mod_symlink_name(256), "@AAE");
    }

    #[test]
    fn the_link_is_shorter_than_the_plain_id() {
        // The whole point of this encoding: the -mod= line cannot grow
        // without bound.
        let id = 3322263460u64;
        assert!(mod_symlink_name(id).len() < format!("@{id}").len());
    }

    #[test]
    fn only_known_url_schemes_are_opened() {
        assert!(open_url("file:///etc/passwd").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn finds_the_workshop_helper_next_to_the_binary() {
        let dir = std::env::temp_dir().join(format!("balota-helper-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        assert!(helper_in(&dir).is_none(), "nothing there yet");

        let helper = dir.join("balota-workshop");
        fs::write(&helper, b"#!/bin/true").unwrap();
        assert_eq!(helper_in(&dir), Some(helper));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn library_lookup_prefers_the_users_steam_over_the_bundled_copy() {
        let base = std::env::temp_dir().join(format!("balota-libs-{}", std::process::id()));
        let steam = base.join("Steam/steamrt64");
        let bundled = base.join("bundle");
        fs::create_dir_all(&steam).unwrap();
        fs::create_dir_all(&bundled).unwrap();

        let env = SteamEnvironment {
            steam_found: true,
            is_flatpak: false,
            steam_running: true,
            steam_root: Some(base.join("Steam").to_string_lossy().to_string()),
            libraries: Vec::new(),
            dayz_dir: None,
            workshop_dir: None,
            dayz_found: false,
            notes: Vec::new(),
        };
        let helper = bundled.join("balota-workshop");

        // Neither copy exists yet, so there is nothing to point at.
        assert!(steam_lib_dirs(&env, &helper).is_empty());

        // Only the bundled one: it gets used (this is the AppImage case).
        fs::write(bundled.join("libsteam_api.so"), b"x").unwrap();
        assert_eq!(steam_lib_dirs(&env, &helper), vec![bundled.clone()]);

        // With Steam's own copy present, that one comes first.
        fs::write(steam.join("libsteam_api.so"), b"x").unwrap();
        assert_eq!(steam_lib_dirs(&env, &helper)[0], steam);

        fs::remove_dir_all(&base).ok();
    }
}
