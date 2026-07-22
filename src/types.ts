export interface SteamEnvironment {
  is_flatpak: boolean;
  steam_root: string;
  dayz_dir: string;
  workshop_dir: string;
  dayz_found: boolean;
}

export interface InstalledModInfo {
  id: string;
  name: string;
  path: string;
  size_bytes: number;
}

export interface DzsaModInfo {
  steamWorkshopId: string;
  name: string;
}

export interface DzsaQueryResult {
  name?: string;
  map?: string;
  players?: number;
  max_players?: number;
  mods?: DzsaModInfo[];
  is_modded?: boolean;
}

export interface ServerItem {
  id: string;
  name: string;
  ip: string;
  port: number;
  query_port: number;
  map: string;
  players: number;
  max_players: number;
  ping: number;
  is_modded: boolean;
  mods_count: number;
  is_favorite?: boolean;
  is_3pp?: boolean;
  has_password?: boolean;
  mods?: DzsaModInfo[];
}

export interface AppUserConfig {
  player_name: string;
  is_flatpak: boolean;
  custom_steam_root?: string;
  use_gamemode: boolean;
  use_mangohud: boolean;
  custom_launch_params: string;
  favorites: string[];
}
