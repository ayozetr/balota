// SPDX-License-Identifier: GPL-3.0-or-later
//! Client for the public dayzsalauncher.com API.
//!
//! Two endpoints:
//!   * `/api/v2/launcher/servers/dayz` — the full master list (around 21,000
//!     servers, ~26 MB uncompressed, ~3 MB gzipped) **including every
//!     server's mod list**.
//!   * `/api/v1/query/<ip>/<query_port>` — single-server lookup, to refresh
//!     player counts and mods right before joining.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::time::Duration;

const MASTER_LIST_URL: &str = "https://dayzsalauncher.com/api/v2/launcher/servers/dayz";
const QUERY_URL: &str = "https://dayzsalauncher.com/api/v1/query";
const USER_AGENT: &str = concat!(
    "Balota/",
    env!("CARGO_PKG_VERSION"),
    " (Linux DayZ launcher)"
);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub ip: String,
    /// **Query** port (the one the API and the A2S protocol use).
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModRef {
    #[serde(rename = "steamWorkshopId", deserialize_with = "flexible_u64")]
    pub id: u64,
    #[serde(default)]
    pub name: String,
}

/// A server as returned by the API. Both endpoints share the same schema, so
/// one type covers them.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiServer {
    pub endpoint: Endpoint,
    /// Port the game connects to. **Not** the same as the query port.
    #[serde(rename = "gamePort", default)]
    pub game_port: u16,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub map: String,
    #[serde(default)]
    pub players: u32,
    #[serde(rename = "maxPlayers", default)]
    pub max_players: u32,
    #[serde(default)]
    pub password: bool,
    #[serde(default)]
    pub version: String,
    #[serde(rename = "firstPersonOnly", default)]
    pub first_person_only: bool,
    #[serde(rename = "battlEye", default)]
    pub battle_eye: bool,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(rename = "timeAcceleration", default)]
    pub time_acceleration: Option<f32>,
    #[serde(default)]
    pub shard: Option<String>,
    #[serde(default)]
    pub mods: Vec<ModRef>,
}

impl ApiServer {
    /// Stable server identifier: `ip:query_port`.
    pub fn id(&self) -> String {
        format!("{}:{}", self.endpoint.ip, self.endpoint.port)
    }

    /// Connection string for `-connect=`, which uses the game port.
    pub fn connect_string(&self) -> String {
        let port = if self.game_port == 0 {
            self.endpoint.port
        } else {
            self.game_port
        };
        format!("{}:{}", self.endpoint.ip, port)
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    status: i32,
    #[serde(default)]
    error: Option<String>,
    result: Option<T>,
}

/// `steamWorkshopId` arrives as a number in the master list, but accepting
/// strings too is cheap insurance: the previous version of this project
/// declared it as `String` and deserialization failed every single time.
fn flexible_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    struct FlexibleU64;

    impl<'de> Visitor<'de> for FlexibleU64 {
        type Value = u64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a mod ID as a number or a string")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> {
            u64::try_from(v).map_err(|_| E::custom("negative mod ID"))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u64, E> {
            Ok(v as u64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.trim()
                .parse()
                .map_err(|_| E::custom("non-numeric mod ID"))
        }
    }

    deserializer.deserialize_any(FlexibleU64)
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .gzip(true)
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Could not build the HTTP client: {e}"))
}

/// Downloads the full master list. Returns the raw body alongside the parsed
/// servers so the disk cache can store it without re-serializing.
pub async fn fetch_master_list() -> Result<(Vec<ApiServer>, String), String> {
    let response = client()?
        .get(MASTER_LIST_URL)
        .send()
        .await
        .map_err(|e| format!("Could not reach dayzsalauncher.com: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "The server list returned HTTP {}",
            response.status()
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Download interrupted: {e}"))?;

    let servers = parse_master_list(&body)?;
    Ok((servers, body))
}

pub fn parse_master_list(body: &str) -> Result<Vec<ApiServer>, String> {
    let envelope: ApiEnvelope<Vec<ApiServer>> =
        serde_json::from_str(body).map_err(|e| format!("Unreadable server list: {e}"))?;

    if let Some(error) = envelope.error {
        return Err(format!("The API answered with an error: {error}"));
    }

    envelope
        .result
        .ok_or_else(|| "The API returned no server list".to_string())
}

/// Single-server lookup, used right before joining so the mod list is current
/// even if the cache is hours old.
pub async fn query_server(ip: &str, query_port: u16) -> Result<ApiServer, String> {
    let url = format!("{QUERY_URL}/{ip}/{query_port}");
    let response = client()?
        .get(&url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Could not query the server: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("The lookup returned HTTP {}", response.status()));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Unreadable reply: {e}"))?;

    let envelope: ApiEnvelope<ApiServer> =
        serde_json::from_str(&body).map_err(|e| format!("Unreadable reply: {e}"))?;

    if envelope.status != 0 || envelope.result.is_none() {
        return Err(envelope
            .error
            .unwrap_or_else(|| "The server did not answer the lookup".to_string()));
    }

    Ok(envelope.result.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_mod_ids_as_numbers_and_as_strings() {
        let body = r#"{
            "status": 0,
            "result": [{
                "endpoint": {"ip": "193.25.252.55", "port": 27016},
                "gamePort": 2302,
                "name": "KarmaKrew Chernarus #1",
                "map": "chernarusplus",
                "players": 115,
                "maxPlayers": 115,
                "password": false,
                "version": "1.29.163451",
                "firstPersonOnly": true,
                "battlEye": true,
                "time": "10:03",
                "mods": [
                    {"name": "Mounts & Sights", "steamWorkshopId": 3322252091},
                    {"name": "Code Lock", "steamWorkshopId": "1564026768"}
                ]
            }]
        }"#;

        let servers = parse_master_list(body).expect("should parse");
        assert_eq!(servers.len(), 1);

        let server = &servers[0];
        assert_eq!(server.id(), "193.25.252.55:27016");
        // The connect port is the game port, not the query port.
        assert_eq!(server.connect_string(), "193.25.252.55:2302");
        assert_eq!(server.max_players, 115);
        assert!(server.first_person_only);
        assert_eq!(server.mods[0].id, 3322252091);
        assert_eq!(server.mods[1].id, 1564026768);
    }

    #[test]
    fn tolerates_missing_fields() {
        let body = r#"{"status":0,"result":[{"endpoint":{"ip":"1.2.3.4","port":27016}}]}"#;
        let servers = parse_master_list(body).expect("should parse");
        assert_eq!(servers[0].players, 0);
        assert!(servers[0].mods.is_empty());
        // With no gamePort we fall back to the query port.
        assert_eq!(servers[0].connect_string(), "1.2.3.4:27016");
    }

    #[test]
    fn surfaces_api_errors() {
        let body = r#"{"status":1,"error":"Timeout has occurred"}"#;
        assert!(parse_master_list(body).is_err());
    }
}
