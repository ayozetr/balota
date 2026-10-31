// SPDX-License-Identifier: GPL-3.0-or-later
/** Mirrors the payloads defined in `src-tauri/src`. */

export interface SteamEnvironment {
  steamFound: boolean;
  isFlatpak: boolean;
  steamRunning: boolean;
  steamRoot: string | null;
  libraries: string[];
  dayzDir: string | null;
  /** Every Workshop folder found, the game's library first. */
  workshopDirs: string[];
  dayzFound: boolean;
  /** Set on a Steam Deck: "Steam Deck (LCD)" or "(OLED)". */
  steamDeck: string | null;
  /** Steam's display name, offered as the in-game name on first run. */
  suggestedName: string | null;
  notes: string[];
}

export interface InstalledMod {
  id: number;
  name: string;
  path: string;
  sizeBytes: number;
}

export interface ServerRow {
  id: string;
  name: string;
  ip: string;
  gamePort: number;
  queryPort: number;
  map: string;
  players: number;
  maxPlayers: number;
  modCount: number;
  firstPersonOnly: boolean;
  password: boolean;
  battleEye: boolean;
  version: string;
  time: string | null;
}

export interface ServerPage {
  items: ServerRow[];
  total: number;
  page: number;
  pageSize: number;
}

export interface ModStatus {
  id: number;
  name: string;
  installed: boolean;
}

export interface ServerDetails {
  server: ServerRow;
  connect: string;
  mods: ModStatus[];
  missingCount: number;
  live: boolean;
  warning: string | null;
}

export interface ItemState {
  id: number;
  /** False means Steam does not track it: a leftover download. */
  subscribed: boolean;
  installed: boolean;
}

export interface ModActionResult {
  count: number;
  /** The IDs Steam actually took. Waiting on the ones it rejected is how the
   *  install dialog used to hang forever. */
  accepted: number[];
  /** Steam took the files but not a subscription — no auto-updates. */
  downloadedOnly: boolean;
  warning: string | null;
}

export interface ModInfo {
  id: number;
  title: string;
  /** Download size in bytes; 0 when Steam does not report one. */
  sizeBytes: number;
  /** The size came from the copy already on disk. */
  installed: boolean;
}

export interface ModInfoReply {
  items: ModInfo[];
  /** IDs Steam knows nothing about — pulled from the Workshop, most likely.
   *  These are the ones that would never finish downloading. */
  unknown: number[];
}

export interface ListStatus {
  total: number;
  fetchedAt: number;
  fromCache: boolean;
  /** Set when the download failed and an older list is shown instead. */
  warning?: string;
}

export interface MapCount {
  map: string;
  servers: number;
}

export interface PingResult {
  id: string;
  online: boolean;
  pingMs: number | null;
  players: number | null;
  maxPlayers: number | null;
}

export interface HistoryEntry {
  id: string;
  name: string;
  map: string;
  lastPlayed: number;
  timesPlayed: number;
}

/** Filters worth remembering between sessions. */
export interface SavedFilters {
  search: string;
  map: string | null;
  mods: string | null;
  perspective: string | null;
  hideEmpty: boolean;
  hideFull: boolean;
  hidePassword: boolean;
  sort: string | null;
  sortDesc: boolean | null;
  tab: string | null;
}

export interface AppConfig {
  playerName: string;
  customSteamRoot: string | null;
  extraLaunchArgs: string;
  favorites: string[];
  history: HistoryEntry[];
  filters: SavedFilters;
}

/**
 * The fields Settings owns. Sending the whole `AppConfig` back meant a
 * keystroke carried a stale `favorites` list with it, undoing a star clicked
 * a moment earlier.
 *
 * `customSteamRoot: null` clears the path; leaving the key out keeps it.
 */
export interface SettingsPatch {
  playerName?: string;
  customSteamRoot?: string | null;
  extraLaunchArgs?: string;
  filters?: SavedFilters;
}

export type ModFilter = "all" | "modded" | "vanilla";
export type Perspective = "all" | "fpp" | "tpp";
export type SortBy = "players" | "name" | "map";
/** Ping is measured client-side, so ordering by it never reaches the backend. */
export type SortColumn = SortBy | "ping";

export interface SortMode {
  by: SortColumn;
  desc: boolean;
}

export interface ServerFilter {
  search: string;
  map: string | null;
  mods: ModFilter;
  perspective: Perspective;
  hideEmpty: boolean;
  hideFull: boolean;
  hidePassword: boolean;
  onlyFavorites: boolean;
  favorites: string[];
  sort: SortBy;
  sortDesc: boolean;
  page: number;
  pageSize: number;
}

export interface LaunchRequest {
  connect?: string;
  modIds: number[];
  dryRun?: boolean;
}

export interface LaunchOutcome {
  command: string;
  linkedMods: string[];
  missingMods: number[];
}
