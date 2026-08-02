// SPDX-License-Identifier: GPL-3.0-or-later
//! Client for the public dayzsalauncher.com API.
//!
//! Two endpoints:
//!   * `/api/v2/launcher/servers/dayz` — the full master list (around 21,000
//!     servers, ~26 MB uncompressed, ~3 MB gzipped) **including every
//!     server's mod list**.
//!   * `/api/v1/query/<ip>/<query_port>` — single-server lookup, to refresh
//!     player counts and mods right before joining.
//!
//! Everything in here is third-party data. `endpoint.ip` in particular ends up
//! on the game's command line, in a DNS lookup and in a URL, so it is parsed as
//! a real address at the door rather than trusted downstream.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

const MASTER_LIST_URL: &str = "https://dayzsalauncher.com/api/v2/launcher/servers/dayz";
const QUERY_URL: &str = "https://dayzsalauncher.com/api/v1/query";
const USER_AGENT: &str = concat!(
    "Balota/",
    env!("CARGO_PKG_VERSION"),
    " (Linux DayZ launcher)"
);

/// The list is ~26 MB. Sixty-four is room to grow several times over and still
/// refuses a decompression bomb: the response is gzipped, so the timeout alone
/// never bounds how much memory a body can turn into.
const MAX_BODY: usize = 64 * 1024 * 1024;
/// A single-server lookup is a few kilobytes.
const MAX_QUERY_BODY: usize = 1024 * 1024;

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

    /// Whether this record can be acted on at all.
    ///
    /// A server whose address is not an address is not a server: that string
    /// would otherwise reach `lookup_host`, a URL path and the game's command
    /// line. Loopback and the wildcard are refused outright — a public list has
    /// no business pointing the launcher at the machine it runs on.
    pub fn is_usable(&self) -> bool {
        match self.endpoint.ip.parse::<IpAddr>() {
            Ok(ip) => {
                !(ip.is_loopback() || ip.is_multicast() || ip.is_unspecified())
                    && self.endpoint.port != 0
            }
            Err(_) => false,
        }
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
            // `as u64` saturates, so NaN and -1.0 both used to become 0 while
            // `visit_i64` rejected the same value.
            if v.is_finite() && v >= 0.0 && v <= u64::MAX as f64 && v.fract() == 0.0 {
                Ok(v as u64)
            } else {
                Err(E::custom("mod ID is not a whole positive number"))
            }
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.trim()
                .parse()
                .map_err(|_| E::custom("non-numeric mod ID"))
        }
    }

    deserializer.deserialize_any(FlexibleU64)
}

/// One client for the whole process: a new one per request meant a fresh TLS
/// handshake every time a server panel was opened.
fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .gzip(true)
                .timeout(Duration::from_secs(60))
                // The API is HTTPS and stays HTTPS; a redirect to plain HTTP,
                // or to any other host, is not something to follow silently.
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| format!("Could not build the HTTP client: {e}"))
        })
        .as_ref()
        .map_err(String::clone)
}

/// Reads a response body with a hard ceiling.
///
/// `Response::text()` has no limit, and with gzip enabled the number of bytes
/// on the wire says nothing about how much memory they become.
async fn read_capped(mut response: reqwest::Response, max: usize) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::new();

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download interrupted: {e}"))?
    {
        if buf.len() + chunk.len() > max {
            return Err(format!(
                "The reply is larger than {} MB, which it should never be. Refusing it.",
                max / (1024 * 1024)
            ));
        }
        buf.extend_from_slice(&chunk);
    }

    String::from_utf8(buf).map_err(|_| "The reply is not valid UTF-8".to_string())
}

/// Waits between attempts when the network is not there yet. Nothing else:
/// once the API answers, an error from it is an answer.
const RETRY_BACKOFF: [u64; 2] = [2, 5];

/// Whether this failure is worth trying again.
///
/// A refused connection or a DNS miss on a laptop that just woke up is worth a
/// second look; an HTTP 500 is not going to change in five seconds.
fn worth_retrying(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_request()
}

/// Downloads the full master list. Returns the raw body alongside the parsed
/// servers so the disk cache can store it without re-serializing.
///
/// Retries the connection a couple of times: the common failure is not a
/// broken API, it is the app starting before Wi-Fi has associated, and the
/// only recovery used to be noticing the error toast and pressing Refresh.
pub async fn fetch_master_list() -> Result<(Vec<ApiServer>, String), String> {
    let mut attempt = 0;

    let response = loop {
        match client()?.get(MASTER_LIST_URL).send().await {
            Ok(response) => break response,
            Err(e) if worth_retrying(&e) && attempt < RETRY_BACKOFF.len() => {
                tokio::time::sleep(Duration::from_secs(RETRY_BACKOFF[attempt])).await;
                attempt += 1;
            }
            Err(e) => {
                return Err(format!(
                    "Could not reach dayzsalauncher.com: {e}. Check your connection and try again."
                ))
            }
        }
    };

    if !response.status().is_success() {
        return Err(format!(
            "The server list returned HTTP {}",
            response.status()
        ));
    }

    let body = read_capped(response, MAX_BODY).await?;
    let servers = parse_master_list(&body)?;
    Ok((servers, body))
}

/// Parses the master list, dropping records rather than the whole list.
///
/// This used to be all-or-nothing: one server with a null endpoint and nobody
/// could see any server at all — including from the disk cache, which is parsed
/// by this same function. The comment on `flexible_u64` records that exact
/// outage happening once already, for one field.
pub fn parse_master_list(body: &str) -> Result<Vec<ApiServer>, String> {
    let envelope: ApiEnvelope<Vec<serde_json::Value>> =
        serde_json::from_str(body).map_err(|e| format!("Unreadable server list: {e}"))?;

    if let Some(error) = envelope.error {
        return Err(format!("The API answered with an error: {error}"));
    }
    if envelope.status != 0 {
        return Err(format!("The API answered with status {}", envelope.status));
    }

    let raw = envelope
        .result
        .ok_or_else(|| "The API returned no server list".to_string())?;
    let total = raw.len();

    let servers: Vec<ApiServer> = raw
        .into_iter()
        .filter_map(|value| serde_json::from_value::<ApiServer>(value).ok())
        .filter(ApiServer::is_usable)
        .collect();

    // Every single record being unreadable is a schema change, not bad luck,
    // and silently showing an empty list would hide it.
    if servers.is_empty() && total > 0 {
        return Err(format!(
            "None of the {total} servers the API returned could be read. The API format has \
             probably changed."
        ));
    }

    Ok(servers)
}

/// Single-server lookup, used right before joining so the mod list is current
/// even if the cache is hours old.
pub async fn query_server(ip: &str, query_port: u16) -> Result<ApiServer, String> {
    // Built out of segments rather than `format!`: an `ip` carrying slashes or
    // a `?` would otherwise rewrite the path of the request.
    let mut url = reqwest::Url::parse(QUERY_URL).map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "Bad API base URL".to_string())?
        .push(ip)
        .push(&query_port.to_string());

    let response = client()?
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Could not query the server: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("The lookup returned HTTP {}", response.status()));
    }

    let body = read_capped(response, MAX_QUERY_BODY).await?;

    let envelope: ApiEnvelope<ApiServer> =
        serde_json::from_str(&body).map_err(|e| format!("Unreadable reply: {e}"))?;

    if envelope.status != 0 {
        return Err(envelope
            .error
            .unwrap_or_else(|| "The server did not answer the lookup".to_string()));
    }

    let server = envelope
        .result
        .ok_or_else(|| "The server did not answer the lookup".to_string())?;

    if !server.is_usable() {
        return Err(format!(
            "The API answered with an address that is not usable ({}:{}).",
            server.endpoint.ip, server.endpoint.port
        ));
    }

    Ok(server)
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

    /// One bad record used to cost the user the entire list, cache included.
    #[test]
    fn a_broken_record_does_not_take_the_list_with_it() {
        let body = r#"{"status":0,"result":[
            {"endpoint":{"ip":"1.2.3.4","port":27016},"name":"Good"},
            {"endpoint":null,"name":"Broken"},
            {"endpoint":{"ip":"5.6.7.8","port":"not a port"},"name":"Also broken"},
            {"name":"No endpoint at all"},
            {"endpoint":{"ip":"9.10.11.12","port":27016},"name":"Also good"}
        ]}"#;

        let servers = parse_master_list(body).expect("should parse");
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Good", "Also good"]);
    }

    /// The address reaches `lookup_host`, a URL path and the game's command
    /// line, so anything that is not an address is dropped at the door.
    #[test]
    fn addresses_that_are_not_addresses_are_dropped() {
        let body = r#"{"status":0,"result":[
            {"endpoint":{"ip":"1.2.3.4:2302 -filePatching -mod=@evil","port":27016},"name":"Injection"},
            {"endpoint":{"ip":"../../admin","port":27016},"name":"Traversal"},
            {"endpoint":{"ip":"127.0.0.1","port":27016},"name":"Loopback"},
            {"endpoint":{"ip":"0.0.0.0","port":27016},"name":"Wildcard"},
            {"endpoint":{"ip":"evil.example.com","port":27016},"name":"Hostname"},
            {"endpoint":{"ip":"1.2.3.4","port":27016},"name":"Fine"}
        ]}"#;

        let servers = parse_master_list(body).expect("should parse");
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Fine"]);
    }

    #[test]
    fn a_list_that_is_entirely_unreadable_is_an_error_not_an_empty_list() {
        let body = r#"{"status":0,"result":[{"endpoint":null},{"endpoint":null}]}"#;
        assert!(parse_master_list(body).is_err());
        // But a genuinely empty list is fine.
        assert_eq!(
            parse_master_list(r#"{"status":0,"result":[]}"#)
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn a_mod_id_that_is_not_a_whole_number_is_rejected() {
        let body = r#"{"status":0,"result":[{
            "endpoint":{"ip":"1.2.3.4","port":27016},
            "mods":[{"steamWorkshopId": -1.0}]
        }]}"#;
        // The record is dropped rather than silently given mod id 0.
        assert!(parse_master_list(body).is_err());
    }
}
