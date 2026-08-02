// SPDX-License-Identifier: GPL-3.0-or-later
//! Steam installation discovery, mod symlink management and game launching.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;

use crate::vdf;

pub const DAYZ_APP_ID: &str = "221100";
pub const FLATPAK_STEAM_ID: &str = "com.valvesoftware.Steam";

/// The name DayZ ships as its placeholder. A good number of community servers
/// kick on sight, so it is never used as a default any more — see `name_error`.
pub const PLACEHOLDER_NAMES: [&str; 3] = ["survivor", "player", "unnamed"];

/// `meta.cpp` is a small text file. Anything larger is not one, and it comes
/// from a Workshop item the server picked, so it does not get to be unbounded.
const MAX_META_BYTES: u64 = 1024 * 1024;
const MAX_META_READ: u64 = 64 * 1024;
/// Mod folders are shallow. A deeper tree means a link loop, not a mod.
const MAX_MOD_DEPTH: usize = 32;
/// VDF files Steam writes are a few KB; a megabyte is already absurd.
const MAX_VDF_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SteamEnvironment {
    pub steam_found: bool,
    pub is_flatpak: bool,
    pub steam_running: bool,
    pub steam_root: Option<String>,
    pub libraries: Vec<String>,
    pub dayz_dir: Option<String>,
    /// Every Workshop folder found, game's library first. Recomputed on use
    /// rather than trusted from here: the folder appears the moment Steam
    /// downloads the first mod, long after detection ran.
    pub workshop_dirs: Vec<String>,
    pub dayz_found: bool,
    /// Set when running on a Steam Deck, so the interface can label controller
    /// buttons the way the hardware does.
    pub steam_deck: Option<String>,
    /// Steam's own display name, offered as the in-game name on first run.
    pub suggested_name: Option<String>,
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
// Small helpers
// ---------------------------------------------------------------------------

/// Reads a text file, refusing anything implausibly large first. Used for the
/// files Steam writes and for Workshop metadata, none of which is trusted.
fn read_text_capped(path: &Path, max: u64) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > max {
        return None;
    }
    let mut content = String::new();
    file.take(max).read_to_string(&mut content).ok()?;
    Some(content)
}

/// Splits a user-supplied parameter string the way a shell would, honouring
/// quotes. `split_whitespace` used to destroy `-profiles="/two words"`.
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;

    for c in input.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(current);
    }
    out
}

/// Why this in-game name will not do, if it will not do.
///
/// An empty name leaves DayZ to invent one, and the placeholders are the exact
/// strings admins kick for. Whitespace is refused because everything after
/// `-applaunch` is reassembled into one command line before the game sees it,
/// so a space would split the name into extra parameters.
pub fn name_error(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Some("Set an in-game name before joining a server.".to_string());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Some(
            "The in-game name cannot contain spaces — DayZ would read the rest as launch \
             parameters. Use an underscore instead."
                .to_string(),
        );
    }
    if PLACEHOLDER_NAMES.contains(&trimmed.to_lowercase().as_str()) {
        return Some(format!(
            "`{trimmed}` is DayZ's placeholder name and a lot of servers kick it on sight. \
             Pick your own name in Settings."
        ));
    }
    None
}

/// Whether this name can be replaced with Steam's without losing a choice the
/// user made.
///
/// Empty, or one of DayZ's placeholders — which earlier versions of Balota
/// shipped as the default, so an existing config carries the exact string that
/// gets its owner kicked. A name that is merely *invalid* (a space in it) is
/// still the user's, and gets an error rather than a silent replacement.
pub fn name_is_replaceable(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty() || PLACEHOLDER_NAMES.contains(&trimmed.to_lowercase().as_str())
}

/// Turns Steam's display name into something DayZ accepts as `-name=`.
fn sanitize_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .filter(|c| !c.is_control())
        .collect();

    let cleaned = cleaned.trim_matches('_').to_string();
    if cleaned.is_empty() || name_error(&cleaned).is_some() {
        None
    } else {
        Some(cleaned)
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Places a Steam installation can live, in order of preference. The boolean
/// marks the Flatpak build.
fn candidate_roots(custom_root: Option<&str>) -> (Vec<(PathBuf, bool)>, Option<String>) {
    let mut out: Vec<(PathBuf, bool)> = Vec::new();
    let mut rejected = None;

    if let Some(custom) = custom_root {
        let custom = expand_tilde(custom.trim());
        if !custom.as_os_str().is_empty() {
            if custom.join("steamapps").is_dir() {
                let flatpak = custom
                    .to_string_lossy()
                    .contains(".var/app/com.valvesoftware.Steam");
                out.push((custom, flatpak));
            } else {
                // Silently ignoring this used to leave the user staring at the
                // same generic "Steam not found" they were trying to fix.
                rejected = Some(format!(
                    "The Steam path you set ({}) has no `steamapps` folder inside, so it was \
                     ignored. Point it at the folder that contains `steamapps`.",
                    custom.display()
                ));
            }
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

    (deduped, rejected)
}

/// `~` is what people type; `PathBuf::from` treats it as a literal directory.
fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(input));
    }
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(input)
}

/// Every library declared in `libraryfolders.vdf`, root included. This is what
/// finds DayZ when it lives on a second drive or on the Steam Deck's SD card.
fn libraries_of(root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![root.to_path_buf()];

    if let Some(content) =
        read_text_capped(&root.join("steamapps/libraryfolders.vdf"), MAX_VDF_BYTES)
    {
        if let Some(folders) = vdf::parse(&content).get("libraryfolders") {
            for (_, entry) in folders.entries() {
                if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                    let path = PathBuf::from(path);
                    // A relative path here would resolve against our own
                    // working directory, which is never what Steam meant.
                    if path.is_absolute() && path.join("steamapps").is_dir() {
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

/// Does this folder actually hold DayZ, or is it just named like it?
fn looks_like_dayz(dir: &Path) -> bool {
    dir.join("DayZ_x64.exe").is_file() || dir.join("addons").is_dir()
}

/// DayZ's folder inside a library. Read from the app manifest because
/// `installdir` is not necessarily the literal string "DayZ".
fn dayz_in_library(library: &Path) -> Option<PathBuf> {
    let steamapps = library.join("steamapps");

    if let Some(content) = read_text_capped(
        &steamapps.join(format!("appmanifest_{DAYZ_APP_ID}.acf")),
        MAX_VDF_BYTES,
    ) {
        if let Some(dir) = vdf::parse(&content)
            .get("AppState")
            .and_then(|v| v.get("installdir"))
            .and_then(|v| v.as_str())
        {
            // `join` with an absolute component throws the prefix away, so an
            // `installdir` of "/home/user" would make that the game folder.
            // Only a single plain component is a legitimate install dir.
            let mut components = Path::new(dir).components();
            let single = matches!(
                (components.next(), components.next()),
                (Some(Component::Normal(_)), None)
            );

            if single {
                let candidate = steamapps.join("common").join(dir);
                if candidate.is_dir()
                    && looks_like_dayz(&candidate)
                    && inside(&candidate, &steamapps)
                {
                    return Some(candidate);
                }
            }
        }
    }

    let fallback = steamapps.join("common/DayZ");
    (fallback.is_dir() && looks_like_dayz(&fallback)).then_some(fallback)
}

/// True when `child` really lives under `parent` once symlinks are resolved.
/// `Path::starts_with` alone is textual and never rejects a `..`.
fn inside(child: &Path, parent: &Path) -> bool {
    match (fs::canonicalize(child), fs::canonicalize(parent)) {
        (Ok(c), Ok(p)) => c.starts_with(p),
        _ => false,
    }
}

fn workshop_in_library(library: &Path) -> Option<PathBuf> {
    let path = library.join("steamapps/workshop/content").join(DAYZ_APP_ID);
    path.is_dir().then_some(path)
}

/// Every Workshop folder that exists **right now**, the game's library first.
///
/// Recomputed on every use on purpose. The folder does not exist until Steam
/// finishes the first download, so a value captured during detection would stay
/// empty for the whole session and make the launcher insist that mods sitting
/// on disk still need downloading.
pub fn workshop_dirs_now(env: &SteamEnvironment) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    // The game's own library first: that is where its mods normally land.
    if let Some(dayz) = &env.dayz_dir {
        if let Some(lib) = Path::new(dayz)
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            if let Some(found) = workshop_in_library(lib) {
                dirs.push(found);
            }
        }
    }

    for library in &env.libraries {
        if let Some(found) = workshop_in_library(Path::new(library)) {
            if !dirs.contains(&found) {
                dirs.push(found);
            }
        }
    }

    dirs
}

/// One Steam installation and what was found inside it.
struct Candidate {
    root: PathBuf,
    is_flatpak: bool,
    libraries: Vec<PathBuf>,
    dayz: Option<PathBuf>,
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

/// Whether this machine is a Steam Deck, and which one.
///
/// Valve's DMI product name is the reliable signal: "Jupiter" for the LCD
/// model, "Galileo" for the OLED. It decides which button labels the
/// controller help should use, since the Deck's own layout is not the same as
/// a generic pad's.
pub fn steam_deck_model() -> Option<String> {
    let name = fs::read_to_string("/sys/devices/virtual/dmi/id/product_name").ok()?;
    match name.trim() {
        "Jupiter" => Some("Steam Deck (LCD)".to_string()),
        "Galileo" => Some("Steam Deck (OLED)".to_string()),
        _ => None,
    }
}

/// Steam's display name for the account that logged in last.
///
/// Saves the user from typing a name on first run, and — more to the point —
/// from being kicked by the servers that refuse DayZ's placeholder.
pub fn steam_persona_name(root: &Path) -> Option<String> {
    let content = read_text_capped(&root.join("config/loginusers.vdf"), MAX_VDF_BYTES)?;
    let parsed = vdf::parse(&content);
    let users = parsed.get("users")?;

    let mut best: Option<(u64, String)> = None;
    for (_, user) in users.entries() {
        let name = user
            .get("PersonaName")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| user.get("AccountName").and_then(|v| v.as_str()))?;

        let most_recent = user.get("MostRecent").and_then(|v| v.as_str()) == Some("1");
        let timestamp = user
            .get("Timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // "Most recent" wins outright; otherwise the newest login does.
        let rank = if most_recent { u64::MAX } else { timestamp };
        if best.as_ref().map(|(r, _)| rank >= *r).unwrap_or(true) {
            best = Some((rank, name.to_string()));
        }
    }

    sanitize_name(&best?.1)
}

pub fn detect(custom_root: Option<&str>) -> SteamEnvironment {
    let mut notes = Vec::new();
    let (roots, rejected) = candidate_roots(custom_root);
    if let Some(rejected) = rejected {
        notes.push(rejected);
    }

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
            workshop_dirs: Vec::new(),
            dayz_found: false,
            steam_deck: steam_deck_model(),
            suggested_name: None,
            notes,
        };
    }

    // Prefer the first installation that actually has DayZ; otherwise fall
    // back to the first one that exists.
    let mut chosen: Option<Candidate> = None;

    for (root, is_flatpak) in &roots {
        let libraries = libraries_of(root);
        let dayz = libraries.iter().find_map(|lib| dayz_in_library(lib));

        let has_dayz = dayz.is_some();
        if chosen.is_none() || has_dayz {
            chosen = Some(Candidate {
                root: root.clone(),
                is_flatpak: *is_flatpak,
                libraries,
                dayz,
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
    } = chosen.expect("the root list is not empty");

    if dayz.is_none() {
        notes.push(format!(
            "Steam found at {}, but DayZ is not installed in any of its {} librar(ies). \
             Install it from Steam, or check that the library is mounted.",
            root.display(),
            libs.len()
        ));
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

    let mut env = SteamEnvironment {
        steam_found: true,
        is_flatpak,
        steam_running,
        steam_root: Some(root.to_string_lossy().to_string()),
        libraries: libs
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        dayz_found: dayz.is_some(),
        steam_deck: steam_deck_model(),
        suggested_name: steam_persona_name(&root),
        dayz_dir: dayz.map(|p| p.to_string_lossy().to_string()),
        workshop_dirs: Vec::new(),
        notes,
    };

    env.workshop_dirs = workshop_dirs_now(&env)
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    if env.dayz_found && env.workshop_dirs.is_empty() {
        env.notes.push(
            "DayZ is installed but the Workshop mod folder does not exist yet. It shows up as \
             soon as you subscribe to your first mod."
                .to_string(),
        );
    }

    env
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

/// Whether a directory entry is a link this app created, rather than something
/// the user put there by hand. Only these are ever deleted.
fn is_our_link_name(name: &str) -> bool {
    // "@" plus base64 of at most 8 bytes: 11 characters, so 12 in total.
    let Some(body) = name.strip_prefix('@') else {
        return false;
    };
    !body.is_empty()
        && body.len() <= 11
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Where a given mod is downloaded, across every Workshop folder.
fn find_mod_dir(workshop_dirs: &[PathBuf], mod_id: u64) -> Option<PathBuf> {
    workshop_dirs
        .iter()
        .map(|dir| dir.join(mod_id.to_string()))
        .find(|path| path.is_dir())
}

/// Creates (or repairs) the mod symlinks inside DayZ's folder and returns the
/// names ready for `-mod=`.
pub fn prepare_mod_symlinks(
    dayz_dir: &Path,
    workshop_dirs: &[PathBuf],
    mod_ids: &[u64],
) -> ModPreparation {
    let mut prep = ModPreparation {
        linked: Vec::new(),
        missing: Vec::new(),
        errors: Vec::new(),
    };

    // The API happily lists the same mod twice; linking it twice would put
    // `@X;@X` on the command line and inflate every count downstream.
    let mut unique: Vec<u64> = Vec::new();
    for &id in mod_ids {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    if unique.is_empty() {
        return prep;
    }

    if workshop_dirs.is_empty() {
        prep.missing.extend_from_slice(&unique);
        return prep;
    }

    for mod_id in unique {
        let Some(source) = find_mod_dir(workshop_dirs, mod_id) else {
            prep.missing.push(mod_id);
            continue;
        };

        let link_name = mod_symlink_name(mod_id);
        let link_path = dayz_dir.join(&link_name);

        // `symlink_metadata` does not follow the link, so it also catches
        // dangling links — which `exists()` reports as missing, making the
        // subsequent create fail with EEXIST.
        match fs::symlink_metadata(&link_path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = fs::read_link(&link_path).ok();
                if target.as_deref() == Some(source.as_path()) {
                    prep.linked.push(link_name);
                    continue;
                }
                // Only links pointing into a Workshop folder are ours to
                // replace. Anything else the user set up by hand stays.
                let ours = target
                    .as_ref()
                    .map(|t| workshop_dirs.iter().any(|dir| t.starts_with(dir)))
                    .unwrap_or(false);
                if !ours {
                    prep.errors.push(format!(
                        "{link_name} already points somewhere else ({}). Remove it by hand if \
                         you want Balota to manage it.",
                        target.map(|t| t.display().to_string()).unwrap_or_default()
                    ));
                    continue;
                }
                if let Err(e) = fs::remove_file(&link_path) {
                    prep.errors
                        .push(format!("Could not recreate link {link_name}: {e}"));
                    continue;
                }
            }
            Ok(meta) if meta.is_dir() => {
                // A real mod folder sitting where the link would go: it works
                // as-is, leave it alone.
                prep.linked.push(link_name);
                continue;
            }
            Ok(_) => {
                // A regular file named like a mod link would make the game
                // load nothing and the server kick with no explanation.
                prep.errors.push(format!(
                    "{link_name} exists but is a file, not a mod folder. Delete it and try again."
                ));
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
/// mod. Returns the names removed, so the interface can say what it did.
///
/// Only links this app could have created, and only ones whose target is
/// genuinely gone: a stat failure because the SD card is unmounted is not a
/// missing mod, and used to cost the user their hand-made links.
pub fn prune_broken_symlinks(dayz_dir: &Path, workshop_dirs: &[PathBuf]) -> Vec<String> {
    let mut removed = Vec::new();
    let Ok(entries) = fs::read_dir(dayz_dir) else {
        return removed;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_our_link_name(name) {
            continue;
        }

        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            continue;
        }

        let Ok(target) = fs::read_link(&path) else {
            continue;
        };
        // Not pointing into a Workshop folder means we did not make it.
        if !workshop_dirs.is_empty() && !workshop_dirs.iter().any(|dir| target.starts_with(dir)) {
            continue;
        }

        match fs::metadata(&target) {
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound && fs::remove_file(&path).is_ok() =>
            {
                removed.push(name.to_string());
            }
            // Present, or unreadable for any other reason: never delete.
            _ => {}
        }
    }

    removed
}

fn mod_name_from_meta(meta_path: &Path) -> Option<String> {
    let content = read_text_capped(meta_path, MAX_META_BYTES.min(MAX_META_READ))?;
    for line in content.lines() {
        let line = line.trim();
        // `starts_with("name")` also matched `nameExtra`; the key is followed
        // by whitespace or `=`.
        let rest = line.strip_prefix("name")?;
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '=') {
            continue;
        }
        let start = rest.find('"')?;
        let rest = &rest[start + 1..];
        let end = rest.find('"')?;
        return Some(rest[..end].chars().take(128).collect());
    }
    None
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        // A link loop, or a mod folder replaced by a link to `/`, would
        // otherwise walk the whole filesystem.
        .follow_links(false)
        .follow_root_links(false)
        .max_depth(MAX_MOD_DEPTH)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn installed_mods(workshop_dirs: &[PathBuf], with_sizes: bool) -> Vec<InstalledMod> {
    let mut mods: Vec<InstalledMod> = Vec::new();
    let mut seen: BTreeSet<u64> = BTreeSet::new();

    for workshop_dir in workshop_dirs {
        let Ok(entries) = fs::read_dir(workshop_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            // Not `is_dir()`: that follows links, and a link here is not a mod.
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let Some(id) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue;
            };
            if !seen.insert(id) {
                continue;
            }

            let name =
                mod_name_from_meta(&path.join("meta.cpp")).unwrap_or_else(|| format!("Mod {id}"));

            mods.push(InstalledMod {
                id,
                name,
                path: path.to_string_lossy().to_string(),
                size_bytes: if with_sizes { dir_size(&path) } else { 0 },
            });
        }
    }

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
    let dayz_dir = env.dayz_dir.as_ref().ok_or(
        "No DayZ installation was found, so there is nothing to launch. Check the Settings tab: \
         it lists every folder that was searched.",
    )?;
    let dayz_dir = Path::new(dayz_dir);
    let workshop_dirs = workshop_dirs_now(env);

    // Validate before touching the filesystem, so a bad name does not leave
    // half the symlinks created.
    let player_name = req.player_name.as_deref().unwrap_or("").trim().to_string();
    if let Some(problem) = name_error(&player_name) {
        return Err(problem);
    }

    // Everything after `-applaunch` is reassembled into a single command line
    // before the game sees it, so an address with a space in it would inject
    // extra parameters. Parsing it as a socket address settles the question.
    let connect = match req
        .connect
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(raw.parse::<SocketAddr>().map_err(|_| {
            format!("`{raw}` is not a valid ip:port address, so the game was not launched.")
        })?),
        None => None,
    };

    let prep = prepare_mod_symlinks(dayz_dir, &workshop_dirs, &req.mod_ids);
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

    if let Some(connect) = connect {
        args.push(format!("-connect={connect}"));
        args.push("-nolauncher".into());
        args.push("-world=empty".into());
    }

    args.push(format!("-name={player_name}"));

    if let Some(extra) = req.extra_args.as_deref() {
        args.extend(split_args(extra));
    }

    let command = format!("{program} {}", args.join(" "));

    if !req.dry_run {
        let mut child = Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not run `{program}`: {e}"))?;

        // `steam -applaunch` hands the request over and exits immediately, so
        // reaping it here costs nothing and keeps a zombie off the process
        // table for as long as Balota stays open.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
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

    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not ask Steam to download the mods: {e}"))?;

    std::thread::spawn(move || {
        let _ = child.wait();
    });

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
        // `.deb` layout: the binary is in `bin/`, the helper in `lib/balota/`.
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
/// The copy shipped next to the helper comes first: inside an AppImage it lives
/// on read-only squashfs and is the one this build was tested against. The
/// user's Steam install is the fallback, which is what lets a `.deb` or a
/// source build work without redistributing Valve's library at all.
fn steam_lib_dirs(env: &SteamEnvironment, helper: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(helper_dir) = helper.parent() {
        dirs.push(helper_dir.to_path_buf());
        // `.deb` layout puts the helper in `lib/balota/`, so the library sits
        // one level up. The old `../lib` resolved to `lib/lib` there — a path
        // that has never existed — and only worked for the AppImage, which is
        // also the only layout the test covered.
        dirs.push(helper_dir.join(".."));
        dirs.push(helper_dir.join("../lib"));
    }

    if let Some(root) = &env.steam_root {
        let root = Path::new(root);
        dirs.push(root.join("steamrt64"));
        dirs.push(root.join("linux64"));
        dirs.push(root.join("ubuntu12_64"));
    }

    dirs.retain(|dir| dir.join("libsteam_api.so").is_file());
    dirs
}

/// `LD_LIBRARY_PATH` for the helper, keeping whatever the environment already
/// had. Replacing it outright breaks the helper when Balota is started from
/// Steam, which puts the Steam Runtime on that path.
fn helper_library_path(lib_dirs: &[PathBuf]) -> Result<std::ffi::OsString, String> {
    let mut dirs: Vec<PathBuf> = lib_dirs.to_vec();
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(dirs).map_err(|e| e.to_string())
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
        .env("LD_LIBRARY_PATH", helper_library_path(&lib_dirs)?)
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
        .env("LD_LIBRARY_PATH", helper_library_path(&lib_dirs)?)
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub installed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModInfoReply {
    #[serde(default)]
    pub items: Vec<ModInfo>,
    /// IDs Steam does not know about — usually pulled from the Workshop. These
    /// are exactly the ones that would never finish downloading.
    #[serde(default)]
    pub unknown: Vec<u64>,
}

/// How large these mods are, before committing to downloading them.
///
/// A 40-mod server can be several gigabytes, and the only way to find that out
/// used to be watching it happen.
pub fn workshop_info(env: &SteamEnvironment, mod_ids: &[u64]) -> Result<ModInfoReply, String> {
    if mod_ids.is_empty() {
        return Ok(ModInfoReply::default());
    }

    let helper = helper_path().ok_or("The balota-workshop helper is missing")?;
    let lib_dirs = steam_lib_dirs(env, &helper);
    if lib_dirs.is_empty() {
        return Err("Could not find libsteam_api.so in the Steam installation".to_string());
    }

    let output = Command::new(helper)
        .arg("info")
        .args(mod_ids.iter().map(u64::to_string))
        .env("LD_LIBRARY_PATH", helper_library_path(&lib_dirs)?)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("Could not ask Steam about these mods: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().last().ok_or("Steam returned nothing")?;

    serde_json::from_str(line).map_err(|e| format!("Unreadable reply from Steam: {e}"))
}

/// Deletes downloaded mod folders outright. Only for items Steam does not
/// track: anything subscribed would simply be restored on the next sync.
///
/// One failure no longer aborts the rest: the user asked for twelve mods to go,
/// and eleven of them going is a better outcome than none.
pub fn delete_mod_folders(workshop_dirs: &[PathBuf], mod_ids: &[u64]) -> Result<usize, String> {
    if workshop_dirs.is_empty() {
        return Err("No Workshop folder detected".to_string());
    }

    let mut removed = 0;
    let mut errors: Vec<String> = Vec::new();

    for id in mod_ids {
        let Some(path) = find_mod_dir(workshop_dirs, *id) else {
            continue;
        };
        // Resolve before deleting: `starts_with` on its own is textual and
        // would happily accept a `..` if an id ever stopped being a number.
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if !workshop_dirs.contains(&base) || !inside(&path, &base) {
            continue;
        }

        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            // Unlinking is all that is wanted here, and all `remove_dir_all`
            // would do anyway.
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
            continue;
        }
        if !meta.is_dir() {
            continue;
        }

        match fs::remove_dir_all(&path) {
            Ok(()) => removed += 1,
            Err(e) => errors.push(format!("mod {id}: {e}")),
        }
    }

    if removed == 0 && !errors.is_empty() {
        return Err(format!("Could not delete any mod ({})", errors.join("; ")));
    }
    Ok(removed)
}

/// Which of these mods are already on disk.
pub fn installed_ids(workshop_dirs: &[PathBuf], mod_ids: &[u64]) -> Vec<u64> {
    mod_ids
        .iter()
        .copied()
        .filter(|id| find_mod_dir(workshop_dirs, *id).is_some())
        .collect()
}

/// Opens a URL in the browser or in the Steam client. Only the schemes and
/// hosts the app actually needs are allowed through.
pub fn open_url(url: &str) -> Result<(), String> {
    // Every call site is a constant or a Workshop id, so the allowlist can be
    // this narrow. It exists so a future bug cannot turn into "open anything".
    const ALLOWED_HOSTS: [&str; 4] = [
        "https://github.com/",
        "https://ko-fi.com/",
        "https://steamcommunity.com/",
        "https://www.gnu.org/",
    ];
    const ALLOWED_STEAM: [&str; 3] = [
        "steam://gameproperties/",
        "steam://open/downloads",
        "steam://url/CommunityFilePage/",
    ];

    let allowed = ALLOWED_HOSTS.iter().chain(ALLOWED_STEAM.iter());
    if !allowed.into_iter().any(|prefix| url.starts_with(prefix)) {
        return Err(format!("URL not allowed: {url}"));
    }

    let mut child = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!("Could not open the URL ({e}). Is `xdg-open` installed on this system?")
        })?;

    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "balota-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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
    fn only_known_urls_are_opened() {
        assert!(open_url("file:///etc/passwd").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
        // Right scheme, wrong host: still refused.
        assert!(open_url("https://evil.example.com/").is_err());
        assert!(open_url("steam://run/730").is_err());
    }

    #[test]
    fn every_link_we_generate_is_recognised_as_ours() {
        for id in [1u64, 255, 256, 1564026768, u64::MAX] {
            let name = mod_symlink_name(id);
            assert!(is_our_link_name(&name), "{name} should be recognised");
        }
        // Hand-made links are not ours to delete.
        assert!(!is_our_link_name("@CF Community"));
        assert!(!is_our_link_name("@"));
        assert!(!is_our_link_name("CF"));
        assert!(!is_our_link_name("@Community Framework"));
        assert!(!is_our_link_name("@toolongtobeoneofours"));
    }

    #[test]
    fn placeholder_and_broken_names_are_refused() {
        assert!(name_error("").is_some());
        assert!(name_error("   ").is_some());
        assert!(name_error("Survivor").is_some());
        assert!(name_error("survivor").is_some());
        // A space would be reassembled into extra launch parameters.
        assert!(name_error("Ayoze Torres").is_some());
        assert!(name_error("Ayoze").is_none());
        assert!(name_error("Ayoze_TR").is_none());
    }

    /// The point of the distinction: a placeholder is Balota's own doing and
    /// gets replaced, a name the user typed is theirs even when it is wrong.
    #[test]
    fn only_placeholders_are_replaced_on_startup() {
        assert!(name_is_replaceable(""));
        assert!(name_is_replaceable("  "));
        assert!(name_is_replaceable("Survivor"));
        assert!(name_is_replaceable("survivor"));

        assert!(!name_is_replaceable("Ayoze"));
        // Invalid, but still the user's: they get an error, not a silent swap.
        assert!(!name_is_replaceable("Ayoze Torres"));
    }

    #[test]
    fn steam_names_become_usable_in_game_names() {
        assert_eq!(
            sanitize_name("Ayoze Torres").as_deref(),
            Some("Ayoze_Torres")
        );
        assert_eq!(sanitize_name("  spaced  ").as_deref(), Some("spaced"));
        // Nothing usable left, and the placeholder is never suggested.
        assert_eq!(sanitize_name("   "), None);
        assert_eq!(sanitize_name("Survivor"), None);
    }

    #[test]
    fn launch_parameters_keep_their_quoted_arguments() {
        assert_eq!(
            split_args(r#"-profiles="/home/a b/prof" -cpuCount=8"#),
            vec!["-profiles=/home/a b/prof", "-cpuCount=8"]
        );
        assert_eq!(
            split_args("  -noSplash   -noPause "),
            vec!["-noSplash", "-noPause"]
        );
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn finds_the_workshop_helper_next_to_the_binary() {
        let dir = scratch("helper");
        assert!(helper_in(&dir).is_none(), "nothing there yet");

        let helper = dir.join("balota-workshop");
        fs::write(&helper, b"#!/bin/true").unwrap();
        assert_eq!(helper_in(&dir), Some(helper));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_bundled_library_wins_over_the_users_steam() {
        let base = scratch("libs");
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
            workshop_dirs: Vec::new(),
            dayz_found: false,
            steam_deck: None,
            suggested_name: None,
            notes: Vec::new(),
        };
        let helper = bundled.join("balota-workshop");

        // Neither copy exists yet, so there is nothing to point at.
        assert!(steam_lib_dirs(&env, &helper).is_empty());

        // Only Steam's own copy: that one gets used (the .deb case).
        fs::write(steam.join("libsteam_api.so"), b"x").unwrap();
        assert_eq!(steam_lib_dirs(&env, &helper), vec![steam.clone()]);

        // With the bundled copy present it comes first: inside an AppImage it
        // is read-only and known to match this build.
        fs::write(bundled.join("libsteam_api.so"), b"x").unwrap();
        assert_eq!(steam_lib_dirs(&env, &helper)[0], bundled);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn an_absolute_installdir_cannot_escape_the_library() {
        let base = scratch("installdir");
        let steamapps = base.join("steamapps");
        let outside = base.join("outside");
        fs::create_dir_all(steamapps.join("common")).unwrap();
        // Make the escape target look exactly like a DayZ install.
        fs::create_dir_all(outside.join("addons")).unwrap();

        fs::write(
            steamapps.join(format!("appmanifest_{DAYZ_APP_ID}.acf")),
            format!(r#""AppState" {{ "installdir" "{}" }}"#, outside.display()),
        )
        .unwrap();

        assert_eq!(dayz_in_library(&base), None, "must not escape the library");

        // The ordinary case still resolves.
        fs::create_dir_all(steamapps.join("common/DayZ/addons")).unwrap();
        fs::write(
            steamapps.join(format!("appmanifest_{DAYZ_APP_ID}.acf")),
            r#""AppState" { "installdir" "DayZ" }"#,
        )
        .unwrap();
        assert_eq!(dayz_in_library(&base), Some(steamapps.join("common/DayZ")));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_folder_named_dayz_without_the_game_is_not_dayz() {
        let base = scratch("notdayz");
        fs::create_dir_all(base.join("steamapps/common/DayZ")).unwrap();
        assert_eq!(dayz_in_library(&base), None);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn pruning_spares_links_the_user_made_and_targets_that_only_look_gone() {
        let base = scratch("prune");
        let dayz = base.join("DayZ");
        let workshop = base.join("workshop");
        fs::create_dir_all(&dayz).unwrap();
        fs::create_dir_all(&workshop).unwrap();

        let live = workshop.join("111");
        fs::create_dir_all(&live).unwrap();

        // Ours, target present: kept.
        std::os::unix::fs::symlink(&live, dayz.join(mod_symlink_name(111))).unwrap();
        // Ours, target gone: removed.
        let dangling = mod_symlink_name(222);
        std::os::unix::fs::symlink(workshop.join("222"), dayz.join(&dangling)).unwrap();
        // The user's own link, pointing outside the Workshop folder: never
        // touched, even though its target is missing too.
        std::os::unix::fs::symlink(base.join("elsewhere/CF"), dayz.join("@CF")).unwrap();

        let removed = prune_broken_symlinks(&dayz, std::slice::from_ref(&workshop));

        assert_eq!(removed, vec![dangling]);
        assert!(
            dayz.join("@CF").symlink_metadata().is_ok(),
            "user link survives"
        );
        assert!(dayz.join(mod_symlink_name(111)).symlink_metadata().is_ok());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_link_pointing_elsewhere_is_reported_instead_of_replaced() {
        let base = scratch("relink");
        let dayz = base.join("DayZ");
        let workshop = base.join("workshop");
        let precious = base.join("precious");
        fs::create_dir_all(&dayz).unwrap();
        fs::create_dir_all(workshop.join("111")).unwrap();
        fs::create_dir_all(&precious).unwrap();

        std::os::unix::fs::symlink(&precious, dayz.join(mod_symlink_name(111))).unwrap();

        let prep = prepare_mod_symlinks(&dayz, std::slice::from_ref(&workshop), &[111]);

        assert!(prep.linked.is_empty());
        assert_eq!(prep.errors.len(), 1, "{:?}", prep.errors);
        assert_eq!(
            fs::read_link(dayz.join(mod_symlink_name(111))).unwrap(),
            precious,
            "the existing link must be left alone"
        );

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn duplicate_mod_ids_are_linked_once() {
        let base = scratch("dupes");
        let dayz = base.join("DayZ");
        let workshop = base.join("workshop");
        fs::create_dir_all(&dayz).unwrap();
        fs::create_dir_all(workshop.join("111")).unwrap();

        let prep = prepare_mod_symlinks(&dayz, &[workshop], &[111, 111, 111]);
        assert_eq!(prep.linked, vec![mod_symlink_name(111)]);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn mods_are_found_across_every_library() {
        let base = scratch("multilib");
        let first = base.join("lib1/workshop");
        let second = base.join("lib2/workshop");
        fs::create_dir_all(first.join("111")).unwrap();
        fs::create_dir_all(second.join("222")).unwrap();

        let dirs = vec![first, second];
        assert_eq!(installed_ids(&dirs, &[111, 222, 333]), vec![111, 222]);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn steam_display_name_is_read_from_loginusers() {
        let base = scratch("persona");
        fs::create_dir_all(base.join("config")).unwrap();
        fs::write(
            base.join("config/loginusers.vdf"),
            r#"
"users"
{
	"76561190000000001"
	{
		"AccountName"		"old"
		"PersonaName"		"Old Account"
		"MostRecent"		"0"
		"Timestamp"		"100"
	}
	"76561190000000002"
	{
		"AccountName"		"ayoze"
		"PersonaName"		"Ayoze TR"
		"MostRecent"		"1"
		"Timestamp"		"50"
	}
}
"#,
        )
        .unwrap();

        // MostRecent wins over the newer timestamp, and the space becomes an
        // underscore so DayZ does not read it as another parameter.
        assert_eq!(steam_persona_name(&base).as_deref(), Some("Ayoze_TR"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_launch_without_a_name_is_refused_before_anything_is_created() {
        let base = scratch("launch");
        let dayz = base.join("DayZ");
        fs::create_dir_all(&dayz).unwrap();

        let env = SteamEnvironment {
            steam_found: true,
            is_flatpak: false,
            steam_running: true,
            steam_root: None,
            libraries: Vec::new(),
            dayz_dir: Some(dayz.to_string_lossy().to_string()),
            workshop_dirs: Vec::new(),
            dayz_found: true,
            steam_deck: None,
            suggested_name: None,
            notes: Vec::new(),
        };

        let request = LaunchRequest {
            connect: Some("1.2.3.4:2302".into()),
            player_name: Some("Survivor".into()),
            dry_run: true,
            ..Default::default()
        };
        assert!(launch(&env, &request).is_err(), "placeholder name refused");

        // A malformed address is refused too, rather than reaching the
        // command line where a space would inject extra parameters.
        let request = LaunchRequest {
            connect: Some("1.2.3.4:2302 -filePatching".into()),
            player_name: Some("Ayoze".into()),
            dry_run: true,
            ..Default::default()
        };
        assert!(launch(&env, &request).is_err(), "bad address refused");

        let request = LaunchRequest {
            connect: Some("1.2.3.4:2302".into()),
            player_name: Some("Ayoze".into()),
            dry_run: true,
            ..Default::default()
        };
        let outcome = launch(&env, &request).expect("should launch");
        assert!(outcome.command.contains("-connect=1.2.3.4:2302"));
        assert!(outcome.command.contains("-name=Ayoze"));

        fs::remove_dir_all(&base).ok();
    }
}
