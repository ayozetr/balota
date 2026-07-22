use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const DAYZ_APP_ID: &str = "221100";
const FLATPAK_STEAM_ID: &str = "com.valvesoftware.Steam";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SteamEnvironment {
    pub is_flatpak: bool,
    pub steam_root: String,
    pub dayz_dir: String,
    pub workshop_dir: String,
    pub dayz_found: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstalledModInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

/// Autodetect Steam installation (Native or Flatpak) and DayZ directories
pub fn detect_steam_env() -> SteamEnvironment {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home"));

    // 1. Check Native Steam
    let native_root = home.join(".local/share/Steam");
    let native_dayz = native_root.join("steamapps/common/DayZ");
    let native_workshop = native_root.join(format!("steamapps/workshop/content/{}", DAYZ_APP_ID));

    if native_dayz.exists() {
        return SteamEnvironment {
            is_flatpak: false,
            steam_root: native_root.to_string_lossy().to_string(),
            dayz_dir: native_dayz.to_string_lossy().to_string(),
            workshop_dir: native_workshop.to_string_lossy().to_string(),
            dayz_found: true,
        };
    }

    // 2. Check Flatpak Steam
    let flatpak_root = home.join(format!(".var/app/{}/data/Steam", FLATPAK_STEAM_ID));
    let flatpak_dayz = flatpak_root.join("steamapps/common/DayZ");
    let flatpak_workshop = flatpak_root.join(format!("steamapps/workshop/content/{}", DAYZ_APP_ID));

    if flatpak_dayz.exists() {
        return SteamEnvironment {
            is_flatpak: true,
            steam_root: flatpak_root.to_string_lossy().to_string(),
            dayz_dir: flatpak_dayz.to_string_lossy().to_string(),
            workshop_dir: flatpak_workshop.to_string_lossy().to_string(),
            dayz_found: true,
        };
    }

    // Fallback default (even if DayZ not yet detected)
    SteamEnvironment {
        is_flatpak: false,
        steam_root: native_root.to_string_lossy().to_string(),
        dayz_dir: native_dayz.to_string_lossy().to_string(),
        workshop_dir: native_workshop.to_string_lossy().to_string(),
        dayz_found: false,
    }
}

/// Convert Mod ID to safe Base64 folder name (e.g. @encoded)
pub fn mod_id_to_symlink_name(mod_id: &str) -> String {
    let bytes = mod_id.as_bytes();
    let encoded = URL_SAFE_NO_PAD.encode(bytes);
    format!("@{}", encoded)
}

/// Ensure symlinks exist in common/DayZ/@<mod_id_base64> pointing to workshop content
pub fn prepare_mod_symlinks(dayz_dir: &str, workshop_dir: &str, mod_ids: &[String]) -> Result<Vec<String>, String> {
    let dayz_path = Path::new(dayz_dir);
    let workshop_path = Path::new(workshop_dir);

    if !dayz_path.exists() {
        return Err(format!("DayZ directory not found: {}", dayz_dir));
    }

    let mut symlink_names = Vec::new();

    for mod_id in mod_ids {
        let mod_workshop_path = workshop_path.join(mod_id);
        if !mod_workshop_path.exists() {
            println!("[ApexDZ] Mod {} not downloaded yet in workshop path", mod_id);
            continue;
        }

        let symlink_name = mod_id_to_symlink_name(mod_id);
        let symlink_target = dayz_path.join(&symlink_name);

        if !symlink_target.exists() {
            #[cfg(unix)]
            {
                if let Err(e) = std::os::unix::fs::symlink(&mod_workshop_path, &symlink_target) {
                    println!("[ApexDZ] Failed to create symlink for mod {}: {}", mod_id, e);
                } else {
                    println!("[ApexDZ] Created symlink {} -> {:?}", symlink_name, mod_workshop_path);
                }
            }
        }
        symlink_names.push(symlink_name);
    }

    Ok(symlink_names)
}

/// Scan installed workshop mods and parse meta.cpp for titles
pub fn get_installed_mods(workshop_dir: &str) -> Vec<InstalledModInfo> {
    let mut mods = Vec::new();
    let path = Path::new(workshop_dir);

    if !path.exists() {
        return mods;
    }

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let mod_path = entry.path();
            if mod_path.is_dir() {
                let mod_id = mod_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let meta_cpp = mod_path.join("meta.cpp");

                let name = if meta_cpp.exists() {
                    parse_mod_name_from_meta(&meta_cpp).unwrap_or_else(|| format!("Mod {}", mod_id))
                } else {
                    format!("Mod {}", mod_id)
                };

                let size = get_dir_size(&mod_path).unwrap_or(0);

                mods.push(InstalledModInfo {
                    id: mod_id,
                    name,
                    path: mod_path.to_string_lossy().to_string(),
                    size_bytes: size,
                });
            }
        }
    }

    mods
}

fn parse_mod_name_from_meta(meta_path: &Path) -> Option<String> {
    if let Ok(content) = fs::read_to_string(meta_path) {
        for line in content.lines() {
            let line_trim = line.trim();
            if line_trim.starts_with("name") {
                if let Some(start) = line_trim.find('"') {
                    if let Some(end) = line_trim[start + 1..].find('"') {
                        return Some(line_trim[start + 1..start + 1 + end].to_string());
                    }
                }
            }
        }
    }
    None
}

fn get_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total_size = 0;
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file() {
            total_size += entry.metadata()?.len();
        }
    }
    Ok(total_size)
}

/// Launch DayZ via Steam with custom arguments
pub fn launch_dayz(
    is_flatpak: bool,
    server_ip_port: Option<&str>,
    player_name: Option<&str>,
    symlink_mods: &[String],
    use_gamemode: bool,
    use_mangohud: bool,
    custom_args: Option<&str>,
) -> Result<String, String> {
    let mut cmd_args = Vec::new();

    if is_flatpak {
        cmd_args.extend(vec![
            "run",
            "--branch=stable",
            "--arch=x86_64",
            "--command=/app/bin/steam-wrapper",
            FLATPAK_STEAM_ID,
            "-applaunch",
            DAYZ_APP_ID,
        ]);
    } else {
        cmd_args.extend(vec!["-applaunch", DAYZ_APP_ID]);
    }

    // Mod string
    if !symlink_mods.is_empty() {
        let mod_arg = format!("-mod={}", symlink_mods.join(";"));
        cmd_args.push(mod_arg);
    }

    // Server connect string
    if let Some(srv) = server_ip_port {
        cmd_args.push(format!("-connect={}", srv));
        cmd_args.push("-nolauncher".to_string());
        cmd_args.push("-world=empty".to_string());
    }

    // Profile name
    if let Some(name) = player_name {
        if !name.trim().is_empty() {
            cmd_args.push(format!("-name={}", name.trim()));
        }
    }

    // Custom user arguments
    if let Some(extra) = custom_args {
        for arg in extra.split_whitespace() {
            cmd_args.push(arg.to_string());
        }
    }

    let executable = if is_flatpak { "flatpak" } else { "steam" };

    // Apply gamemoderun or mangohud wrapper if enabled
    let mut final_cmd = Command::new(executable);
    
    if use_gamemode {
        if let Ok(path) = which_command("gamemoderun") {
            final_cmd = Command::new(path);
            final_cmd.arg(executable);
        }
    } else if use_mangohud {
        if let Ok(path) = which_command("mangohud") {
            final_cmd = Command::new(path);
            final_cmd.arg(executable);
        }
    }

    final_cmd.args(&cmd_args);

    println!("[ApexDZ Launch] Executing: {:?} {:?}", final_cmd.get_program(), cmd_args);

    match final_cmd.spawn() {
        Ok(_) => Ok(format!("DayZ launched successfully! Command: {} {}", executable, cmd_args.join(" "))),
        Err(e) => Err(format!("Failed to launch Steam/DayZ: {}", e)),
    }
}

fn which_command(cmd: &str) -> Result<String, ()> {
    let output = Command::new("which").arg(cmd).output();
    if let Ok(out) = output {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }
    Err(())
}
