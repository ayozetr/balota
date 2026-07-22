/** Typed wrappers around the Tauri commands. */

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  InstalledMod,
  LaunchOutcome,
  LaunchRequest,
  ListStatus,
  MapCount,
  PingResult,
  ServerDetails,
  ServerFilter,
  ServerPage,
  SteamEnvironment,
} from "./types";

export const steamEnvironment = (refresh = false) =>
  invoke<SteamEnvironment>("steam_environment", { refresh });

export const refreshServers = (force = false) =>
  invoke<ListStatus>("refresh_servers", { force });

export const queryServers = (filter: ServerFilter) =>
  invoke<ServerPage>("query_servers", { filter });

export const serverMaps = () => invoke<MapCount[]>("server_maps");

export const serverDetails = (id: string, refresh = true) =>
  invoke<ServerDetails>("server_details", { id, refresh });

export const pingServers = (ids: string[], timeoutMs?: number) =>
  invoke<PingResult[]>("ping_servers", { ids, timeoutMs });

export const installedMods = (withSizes = false) =>
  invoke<InstalledMod[]>("installed_mods", { withSizes });

export const subscribeMods = (ids: number[]) =>
  invoke<number>("subscribe_mods", { ids });

export const pruneSymlinks = () => invoke<number>("prune_symlinks");

export const launchGame = (request: LaunchRequest) =>
  invoke<LaunchOutcome>("launch_game", { request });

export const openUrl = (url: string) => invoke<void>("open_url", { url });

export const getConfig = () => invoke<AppConfig>("get_config");

export const saveConfig = (config: AppConfig) =>
  invoke<AppConfig>("save_config", { config });

export const toggleFavorite = (id: string) =>
  invoke<AppConfig>("toggle_favorite", { id });

export const recordLaunch = (id: string, name: string, map: string) =>
  invoke<AppConfig>("record_launch", { id, name, map });

/** Steam's own properties dialog, where launch options are edited. */
export const openSteamProperties = () =>
  openUrl("steam://gameproperties/221100");

export const openWorkshopPage = (modId: number) =>
  openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${modId}`);

/**
 * `navigator.clipboard` needs a secure context and is not always available
 * inside the webview, so fall back to the old textarea trick.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(textarea);
      return ok;
    } catch {
      return false;
    }
  }
}
