// SPDX-License-Identifier: GPL-3.0-or-later
//! A2S_INFO queries (Valve's UDP protocol) to measure real latency and refresh
//! player counts.
//!
//! Since 2020 servers require a *challenge*: the first query is answered with
//! `0xFFFFFFFF 'A' <4 bytes>` and the query has to be repeated with those four
//! bytes appended. Without that you never get any data, just a 9-byte packet
//! that is easy to mistake for a valid reply.

use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;

const A2S_INFO_HEADER: &[u8] = b"\xFF\xFF\xFF\xFFTSource Engine Query\0";
const SINGLE_PACKET: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const MULTI_PACKET: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFE];

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct A2sInfo {
    pub ping_ms: u32,
    pub name: Option<String>,
    pub map: Option<String>,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    /// `ip:query_port`, so the frontend can match it to a table row.
    pub id: String,
    pub online: bool,
    /// `None` when the server did not answer within the timeout.
    pub ping_ms: Option<u32>,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
}

/// Sequential reader over the response fields.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let byte = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn u16_le(&mut self) -> Option<u16> {
        let bytes = self.buf.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn cstring(&mut self) -> Option<String> {
        let start = self.pos;
        let end = self.buf[start..].iter().position(|&b| b == 0)? + start;
        self.pos = end + 1;
        Some(String::from_utf8_lossy(&self.buf[start..end]).into_owned())
    }
}

/// Parses the payload of an A2S_INFO reply (everything after the four header
/// bytes).
fn parse_info(payload: &[u8], ping_ms: u32) -> Option<A2sInfo> {
    let mut reader = Reader::new(payload);

    if reader.u8()? != b'I' {
        return None;
    }
    let _protocol = reader.u8()?;

    let name = reader.cstring()?;
    let map = reader.cstring()?;
    let _folder = reader.cstring()?;
    // On DayZ this field is not the game name but the description the admin
    // configured (often a Discord URL). It doesn't matter: what matters is
    // consuming it so the rest of the fields line up.
    let _game = reader.cstring()?;
    // AppID truncated to 16 bits. DayZ's (221100) doesn't fit, so 0 arrives.
    let _app_id = reader.u16_le()?;

    let players = reader.u8()? as u32;
    let max_players = reader.u8()? as u32;
    let _bots = reader.u8()?;
    let _server_type = reader.u8()?;
    let _environment = reader.u8()?;
    let _visibility = reader.u8()?;
    let _vac = reader.u8()?;
    let version = reader.cstring();

    Some(A2sInfo {
        ping_ms,
        name: Some(name),
        map: Some(map),
        players: Some(players),
        max_players: Some(max_players),
        version,
    })
}

/// Queries one server, solving the challenge when required.
pub async fn query(ip: &str, query_port: u16, wait: Duration) -> Result<A2sInfo, String> {
    let mut addrs = tokio::net::lookup_host((ip, query_port))
        .await
        .map_err(|e| format!("Address not resolvable: {e}"))?;
    let addr = addrs.next().ok_or("Address not resolvable")?;

    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("Could not open UDP socket: {e}"))?;
    socket
        .connect(addr)
        .await
        .map_err(|e| format!("Could not connect the socket: {e}"))?;

    let started = Instant::now();
    let mut request = A2S_INFO_HEADER.to_vec();
    let mut buf = [0u8; 4096];
    // Latency is measured on the first round trip, which is the one that
    // reflects network distance; the challenge would add a second hop.
    let mut ping_ms: Option<u32> = None;

    // One initial attempt plus two retries: some servers issue a fresh
    // challenge even after being answered.
    for _ in 0..3 {
        socket
            .send(&request)
            .await
            .map_err(|e| format!("Could not send the query: {e}"))?;

        let received = timeout(wait, socket.recv(&mut buf))
            .await
            .map_err(|_| "No reply (timed out)".to_string())?
            .map_err(|e| format!("Receive error: {e}"))?;

        let elapsed = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
        ping_ms.get_or_insert(elapsed);

        if received < 5 {
            return Err("Reply too short".into());
        }

        let header = &buf[..4];
        if header == MULTI_PACKET {
            // Split response. Extremely rare for A2S_INFO, and reassembling it
            // buys us nothing here: the server is alive and latency is already
            // measured.
            return Err("Split responses are not supported".into());
        }
        if header != SINGLE_PACKET {
            return Err("Unknown header".into());
        }

        let payload = &buf[4..received];

        match payload.first() {
            // Challenge: repeat the query with the four bytes appended.
            Some(b'A') if payload.len() >= 5 => {
                request = A2S_INFO_HEADER.to_vec();
                request.extend_from_slice(&payload[1..5]);
            }
            Some(b'I') => {
                let ping = ping_ms.unwrap_or(elapsed);
                return parse_info(payload, ping).ok_or_else(|| "Unreadable reply".to_string());
            }
            _ => return Err("Unexpected reply".into()),
        }
    }

    Err("Server kept asking for a challenge".into())
}

/// Queries many servers at once. Meant for the visible page of the table, not
/// for all 21,000 servers in the list.
pub async fn ping_many(targets: Vec<(String, u16)>, wait_ms: u64) -> Vec<PingResult> {
    use tokio::sync::Semaphore;

    let permits = std::sync::Arc::new(Semaphore::new(64));
    let wait = Duration::from_millis(wait_ms.clamp(200, 5_000));
    let mut tasks = tokio::task::JoinSet::new();

    for (ip, port) in targets {
        let permits = permits.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await.ok();
            let id = format!("{ip}:{port}");
            match query(&ip, port, wait).await {
                Ok(info) => PingResult {
                    id,
                    online: true,
                    ping_ms: Some(info.ping_ms),
                    players: info.players,
                    max_players: info.max_players,
                },
                Err(_) => PingResult {
                    id,
                    online: false,
                    ping_ms: None,
                    players: None,
                    max_players: None,
                },
            }
        });
    }

    let mut results = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        if let Ok(result) = joined {
            results.push(result);
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real reply captured from a DayZ server (KarmaKrew, 115/115).
    fn real_reply() -> Vec<u8> {
        let mut packet = Vec::new();
        packet.push(b'I');
        packet.push(0x11); // protocol
        packet.extend_from_slice(b"KarmaKrew Chernarus #1 EU - 1PP | VANILLA + MODS\0");
        packet.extend_from_slice(b"chernarusplus\0");
        packet.extend_from_slice(b"dayz\0");
        packet.extend_from_slice(b"https://www.discord.gg/KarmaKrew\0");
        packet.extend_from_slice(&[0x00, 0x00]); // truncated appid
        packet.push(115); // players
        packet.push(115); // max players
        packet.push(0); // bots
        packet.push(b'd'); // server type
        packet.push(b'w'); // operating system
        packet.push(0); // visibility
        packet.push(1); // VAC
        packet.extend_from_slice(b"1.29.163451\0");
        packet
    }

    #[test]
    fn parses_player_counts_from_a_real_reply() {
        let info = parse_info(&real_reply(), 59).expect("should parse");
        assert_eq!(
            info.name.as_deref(),
            Some("KarmaKrew Chernarus #1 EU - 1PP | VANILLA + MODS")
        );
        assert_eq!(info.map.as_deref(), Some("chernarusplus"));
        assert_eq!(info.players, Some(115));
        assert_eq!(info.max_players, Some(115));
        assert_eq!(info.version.as_deref(), Some("1.29.163451"));
        assert_eq!(info.ping_ms, 59);
    }

    #[test]
    fn does_not_mistake_a_challenge_for_data() {
        // This is exactly what a server sends back on the first query: five
        // payload bytes that carry no information at all.
        let challenge = [b'A', 0x11, 0x22, 0x33, 0x44];
        assert!(parse_info(&challenge, 10).is_none());
    }

    #[test]
    fn never_reads_past_the_end_of_a_truncated_reply() {
        let full = real_reply();
        for cut in 0..full.len() {
            // Must not panic on any prefix of the reply.
            let _ = parse_info(&full[..cut], 1);
        }
    }
}
