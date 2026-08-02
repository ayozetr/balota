// SPDX-License-Identifier: GPL-3.0-or-later
//! In-memory store for the master server list, with disk cache, filtering and
//! pagination.
//!
//! Filtering happens here rather than in the frontend on purpose: there are
//! around 21,000 servers, and shipping them all across the IPC bridge on every
//! keystroke would freeze the UI.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::dzsa::{self, ApiServer};

/// How long the disk cache is considered fresh.
const CACHE_TTL_SECS: u64 = 15 * 60;
/// The list is ~26 MB; a cache file far past that is not one of ours.
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListStatus {
    pub total: usize,
    /// Download time, in seconds since the Unix epoch.
    pub fetched_at: u64,
    pub from_cache: bool,
}

/// Trimmed-down server record: what the table needs, without dragging along
/// every server's mod list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRow {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub game_port: u16,
    pub query_port: u16,
    pub map: String,
    pub players: u32,
    pub max_players: u32,
    pub mod_count: usize,
    pub first_person_only: bool,
    pub password: bool,
    pub battle_eye: bool,
    pub version: String,
    pub time: Option<String>,
}

/// A server plus the strings the filter and the sort would otherwise rebuild
/// on every keystroke.
///
/// Sorting 21,000 servers by name used to allocate a lowercase copy of every
/// name O(n log n) times, and the map filter formatted the pretty name twice
/// per comparison. Computing them once, when the list arrives, is the whole
/// difference between a responsive search box and a stuttering one.
#[derive(Debug, Clone)]
pub struct StoredServer {
    pub api: ApiServer,
    pub id: String,
    pub name_lower: String,
    pub map_pretty: String,
    pub map_lower: String,
}

impl From<ApiServer> for StoredServer {
    fn from(api: ApiServer) -> Self {
        let map_pretty = pretty_map(&api.map);
        Self {
            id: api.id(),
            name_lower: api.name.to_lowercase(),
            map_lower: map_pretty.to_lowercase(),
            map_pretty,
            api,
        }
    }
}

impl From<&StoredServer> for ServerRow {
    fn from(stored: &StoredServer) -> Self {
        let server = &stored.api;
        Self {
            id: stored.id.clone(),
            name: server.name.clone(),
            ip: server.endpoint.ip.clone(),
            game_port: if server.game_port == 0 {
                server.endpoint.port
            } else {
                server.game_port
            },
            query_port: server.endpoint.port,
            map: stored.map_pretty.clone(),
            players: server.players,
            max_players: server.max_players,
            mod_count: server.mods.len(),
            first_person_only: server.first_person_only,
            password: server.password,
            battle_eye: server.battle_eye,
            version: server.version.clone(),
            time: server.time.clone(),
        }
    }
}

impl From<&ApiServer> for ServerRow {
    fn from(server: &ApiServer) -> Self {
        Self::from(&StoredServer::from(server.clone()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPage {
    pub items: Vec<ServerRow>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCount {
    pub map: String,
    pub servers: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ModFilter {
    #[default]
    All,
    Modded,
    Vanilla,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Perspective {
    #[default]
    All,
    /// First person only (1PP).
    Fpp,
    /// Third person allowed (3PP).
    Tpp,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    #[default]
    Players,
    Name,
    Map,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerFilter {
    pub search: String,
    /// Display name of the map, exactly as shown in the selector.
    pub map: Option<String>,
    pub mods: ModFilter,
    pub perspective: Perspective,
    pub hide_empty: bool,
    pub hide_full: bool,
    pub hide_password: bool,
    /// When set, only IDs present in `favorites` are returned.
    pub only_favorites: bool,
    pub favorites: Vec<String>,
    pub sort: SortBy,
    /// Reverses the column's natural order. Players counts down by default,
    /// names and maps count up.
    pub sort_desc: Option<bool>,
    pub page: usize,
    pub page_size: usize,
}

impl ServerFilter {
    /// Descending unless asked otherwise, per column.
    fn descending(&self) -> bool {
        self.sort_desc
            .unwrap_or(matches!(self.sort, SortBy::Players))
    }
}

/// Internal map names translated to the ones the community uses.
pub fn pretty_map(raw: &str) -> String {
    let key = raw.trim().to_lowercase();
    let known = match key.as_str() {
        "chernarusplus" | "chernarus" => "Chernarus",
        "enoch" => "Livonia",
        "sakhal" => "Sakhal",
        "namalsk" => "Namalsk",
        "deerisle" => "Deer Isle",
        "banov" => "Banov",
        "esseker" => "Esseker",
        "chiemsee" => "Chiemsee",
        "pripyat" => "Pripyat",
        "rostow" => "Rostow",
        "takistanplus" => "Takistan",
        "valning" => "Valning",
        "alteria" => "Alteria",
        "" => "Unknown",
        _ => {
            // Community maps: capitalize and move on.
            let mut chars = key.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Unknown".to_string(),
            };
        }
    };
    known.to_string()
}

#[derive(Debug, Default)]
pub struct ServerStore {
    servers: Vec<StoredServer>,
    fetched_at: u64,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path() -> Option<PathBuf> {
    let dir = dirs::cache_dir()?.join("balota");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("servers.json"))
}

/// Reads the cached body if it is fresh and plausible. Returns the body and
/// the time it was written.
///
/// Separate from the store so the caller can do this on a blocking thread
/// without holding the lock across 26 MB of file I/O and JSON parsing.
pub fn read_cache() -> Option<(String, u64)> {
    let path = cache_path()?;
    let meta = fs::metadata(&path).ok()?;
    if meta.len() > MAX_CACHE_BYTES {
        // A body this size can only have got here by being written by
        // something that is not us, or by an API having a very bad day.
        let _ = fs::remove_file(&path);
        return None;
    }

    let modified = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();

    if now_secs().saturating_sub(modified) > CACHE_TTL_SECS {
        return None;
    }

    Some((fs::read_to_string(&path).ok()?, modified))
}

/// Writes the cache atomically. A half-written file used to be left behind for
/// the next start to trip over.
pub fn write_cache(body: &str) {
    let Some(path) = cache_path() else {
        return;
    };
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));

    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &path)
    };

    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        eprintln!("balota: could not cache the server list: {e}");
    }
}

impl ServerStore {
    pub fn status(&self) -> ListStatus {
        ListStatus {
            total: self.servers.len(),
            fetched_at: self.fetched_at,
            from_cache: false,
        }
    }

    /// Swaps in an already-parsed list. Parsing happens off the async runtime,
    /// so the lock is only held for the length of a move.
    pub fn adopt(
        &mut self,
        servers: Vec<ApiServer>,
        fetched_at: u64,
        from_cache: bool,
    ) -> ListStatus {
        self.servers = servers.into_iter().map(StoredServer::from).collect();
        self.fetched_at = fetched_at;

        ListStatus {
            total: self.servers.len(),
            fetched_at,
            from_cache,
        }
    }

    pub fn find(&self, id: &str) -> Option<&ApiServer> {
        self.servers
            .iter()
            .find(|s| s.id == id)
            .map(|stored| &stored.api)
    }

    /// Maps present in the list, ordered by server count.
    pub fn maps(&self) -> Vec<MapCount> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for server in &self.servers {
            *counts.entry(server.map_pretty.as_str()).or_default() += 1;
        }

        let mut maps: Vec<MapCount> = counts
            .into_iter()
            .map(|(map, servers)| MapCount {
                map: map.to_string(),
                servers,
            })
            .collect();
        maps.sort_by(|a, b| b.servers.cmp(&a.servers).then_with(|| a.map.cmp(&b.map)));
        maps
    }

    pub fn query(&self, filter: &ServerFilter) -> ServerPage {
        let needle = filter.search.trim().to_lowercase();
        let favorites: HashSet<&str> = filter.favorites.iter().map(String::as_str).collect();

        let mut matched: Vec<&StoredServer> = self
            .servers
            .iter()
            .filter(|stored| {
                let server = &stored.api;
                if filter.only_favorites && !favorites.contains(stored.id.as_str()) {
                    return false;
                }
                if !needle.is_empty() {
                    let matches = stored.name_lower.contains(&needle)
                        || server.endpoint.ip.contains(&needle)
                        || stored.map_lower.contains(&needle);
                    if !matches {
                        return false;
                    }
                }
                if let Some(map) = &filter.map {
                    if !map.is_empty() && &stored.map_pretty != map {
                        return false;
                    }
                }
                match filter.mods {
                    ModFilter::All => {}
                    ModFilter::Modded if server.mods.is_empty() => return false,
                    ModFilter::Vanilla if !server.mods.is_empty() => return false,
                    _ => {}
                }
                match filter.perspective {
                    Perspective::All => {}
                    Perspective::Fpp if !server.first_person_only => return false,
                    Perspective::Tpp if server.first_person_only => return false,
                    _ => {}
                }
                if filter.hide_empty && server.players == 0 {
                    return false;
                }
                if filter.hide_full
                    && server.max_players > 0
                    && server.players >= server.max_players
                {
                    return false;
                }
                if filter.hide_password && server.password {
                    return false;
                }
                true
            })
            .collect();

        // Sorted ascending first, then reversed if asked: one comparator per
        // column instead of two.
        match filter.sort {
            SortBy::Players => matched.sort_by(|a, b| {
                a.api
                    .players
                    .cmp(&b.api.players)
                    .then_with(|| b.name_lower.cmp(&a.name_lower))
            }),
            SortBy::Name => matched.sort_by(|a, b| a.name_lower.cmp(&b.name_lower)),
            SortBy::Map => matched.sort_by(|a, b| {
                a.map_pretty
                    .cmp(&b.map_pretty)
                    .then_with(|| b.api.players.cmp(&a.api.players))
            }),
        }
        if filter.descending() {
            matched.reverse();
        }

        let page_size = filter.page_size.clamp(10, 200);
        let total = matched.len();
        let start = filter.page.saturating_mul(page_size);

        let items = matched
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(ServerRow::from)
            .collect();

        ServerPage {
            items,
            total,
            page: filter.page,
            page_size,
        }
    }
}

/// Parses a master-list body into servers. Kept here so the caller can run it
/// on a blocking thread.
pub fn parse(body: &str) -> Result<Vec<ApiServer>, String> {
    dzsa::parse_master_list(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dzsa::{Endpoint, ModRef};

    fn server(name: &str, players: u32, max: u32, map: &str, mods: usize, fpp: bool) -> ApiServer {
        ApiServer {
            endpoint: Endpoint {
                ip: format!("10.0.0.{}", name.len()),
                port: 27016,
            },
            game_port: 2302,
            name: name.to_string(),
            map: map.to_string(),
            players,
            max_players: max,
            password: false,
            version: "1.29".to_string(),
            first_person_only: fpp,
            battle_eye: true,
            time: None,
            time_acceleration: None,
            shard: None,
            mods: (0..mods)
                .map(|i| ModRef {
                    id: 1000 + i as u64,
                    name: format!("Mod {i}"),
                })
                .collect(),
        }
    }

    fn store() -> ServerStore {
        let mut store = ServerStore::default();
        store.adopt(
            vec![
                server("Vanilla Chernarus", 40, 60, "chernarusplus", 0, true),
                server("Modded Livonia", 0, 40, "enoch", 12, false),
                server("Full Sakhal", 60, 60, "sakhal", 3, false),
            ],
            0,
            false,
        );
        store
    }

    fn base_filter() -> ServerFilter {
        ServerFilter {
            page_size: 50,
            ..Default::default()
        }
    }

    #[test]
    fn filters_by_mods_and_perspective() {
        let store = store();

        let modded_only = ServerFilter {
            mods: ModFilter::Modded,
            ..base_filter()
        };
        assert_eq!(store.query(&modded_only).total, 2);

        let vanilla_only = ServerFilter {
            mods: ModFilter::Vanilla,
            ..base_filter()
        };
        let page = store.query(&vanilla_only);
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "Vanilla Chernarus");

        let fpp_only = ServerFilter {
            perspective: Perspective::Fpp,
            ..base_filter()
        };
        assert_eq!(store.query(&fpp_only).total, 1);
    }

    #[test]
    fn hides_empty_and_full_servers() {
        let store = store();

        let no_empty = ServerFilter {
            hide_empty: true,
            ..base_filter()
        };
        assert_eq!(store.query(&no_empty).total, 2);

        let no_full = ServerFilter {
            hide_full: true,
            ..base_filter()
        };
        let page = store.query(&no_full);
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|s| s.name != "Full Sakhal"));
    }

    #[test]
    fn searches_by_name_ip_and_map() {
        let store = store();

        let by_map = ServerFilter {
            search: "livonia".into(),
            ..base_filter()
        };
        assert_eq!(store.query(&by_map).total, 1);

        let by_name = ServerFilter {
            search: "SAKHAL".into(),
            ..base_filter()
        };
        assert_eq!(store.query(&by_name).total, 1);
    }

    #[test]
    fn sorts_by_players_and_paginates() {
        let store = store();

        let page = store.query(&ServerFilter {
            page_size: 10,
            ..base_filter()
        });
        assert_eq!(page.items[0].name, "Full Sakhal");
        assert_eq!(page.items[0].players, 60);

        // Asking for a second page of ten leaves nothing behind.
        let second = store.query(&ServerFilter {
            page: 1,
            page_size: 10,
            ..base_filter()
        });
        assert_eq!(second.total, 3);
        assert!(second.items.is_empty());
    }

    /// Clicking a column header twice has to actually reverse the order — the
    /// arrow in the table said it did long before it was true.
    #[test]
    fn every_column_can_be_reversed() {
        let store = store();

        let fewest_first = store.query(&ServerFilter {
            sort: SortBy::Players,
            sort_desc: Some(false),
            ..base_filter()
        });
        assert_eq!(fewest_first.items[0].players, 0);

        let by_name = store.query(&ServerFilter {
            sort: SortBy::Name,
            ..base_filter()
        });
        assert_eq!(by_name.items[0].name, "Full Sakhal");

        let by_name_desc = store.query(&ServerFilter {
            sort: SortBy::Name,
            sort_desc: Some(true),
            ..base_filter()
        });
        assert_eq!(by_name_desc.items[0].name, "Vanilla Chernarus");
    }

    #[test]
    fn the_connect_port_is_not_the_query_port() {
        let page = store().query(&base_filter());
        assert_eq!(page.items[0].query_port, 27016);
        assert_eq!(page.items[0].game_port, 2302);
    }

    #[test]
    fn translates_internal_map_names() {
        assert_eq!(pretty_map("chernarusplus"), "Chernarus");
        assert_eq!(pretty_map("enoch"), "Livonia");
        assert_eq!(pretty_map("onforin"), "Onforin");
        assert_eq!(pretty_map(""), "Unknown");
    }
}
