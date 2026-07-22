/** Mirrors the payloads defined in `src-tauri/src`. */

export interface SteamEnvironment {
  steamFound: boolean;
  isFlatpak: boolean;
  steamRunning: boolean;
  steamRoot: string | null;
  libraries: string[];
  dayzDir: string | null;
  workshopDir: string | null;
  dayzFound: boolean;
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
  /** Steam took the files but not a subscription — no auto-updates. */
  downloadedOnly: boolean;
  warning: string | null;
}

export interface ListStatus {
  total: number;
  fetchedAt: number;
  fromCache: boolean;
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

export interface AppConfig {
  playerName: string;
  customSteamRoot: string | null;
  extraLaunchArgs: string;
  favorites: string[];
  history: HistoryEntry[];
}

export type ModFilter = "all" | "modded" | "vanilla";
export type Perspective = "all" | "fpp" | "tpp";
export type SortBy = "players" | "name" | "map";

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
  page: number;
  pageSize: number;
}

export interface LaunchRequest {
  connect?: string;
  modIds: number[];
  playerName?: string;
  extraArgs?: string;
  dryRun?: boolean;
}

export interface LaunchOutcome {
  command: string;
  linkedMods: string[];
  missingMods: number[];
}
