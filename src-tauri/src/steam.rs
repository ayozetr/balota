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
}
