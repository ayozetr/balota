// SPDX-License-Identifier: GPL-3.0-or-later
//! A2S queries (Valve's UDP protocol) to measure real latency, refresh player
//! counts and, when dayzsalauncher.com is down, read a server's details and mod
//! list straight from the server itself.
//!
//! Since 2020 servers require a *challenge*: the first query is answered with
//! `0xFFFFFFFF 'A' <4 bytes>` and the query has to be repeated with those four
//! bytes appended. Without that you never get any data, just a 9-byte packet
//! that is easy to mistake for a valid reply.

use serde::Serialize;

use crate::dzsa::{ApiServer, Endpoint, ModRef};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;

const A2S_INFO_HEADER: &[u8] = b"\xFF\xFF\xFF\xFFTSource Engine Query\0";
const A2S_RULES_HEADER: &[u8] = b"\xFF\xFF\xFF\xFFV";
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
    /// Port the game connects to, from the extra data flags. On DayZ it is
    /// not the query port.
    #[serde(skip)]
    pub game_port: Option<u16>,
    #[serde(skip)]
    pub password: Option<bool>,
    /// DayZ packs its settings in here: `battleye`, `no3rd`, the clock…
    #[serde(skip)]
    pub keywords: Option<String>,
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

    fn bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        let bytes = self.buf.get(self.pos..self.pos.checked_add(len)?)?;
        self.pos += len;
        Some(bytes)
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
    let visibility = reader.u8()?;
    let _vac = reader.u8()?;
    let version = reader.cstring();

    // Extra data: optional, and only the fields before the ones we want have
    // to be walked over. A reply cut short just leaves them empty.
    let mut game_port = None;
    let mut keywords = None;
    if let Some(edf) = reader.u8() {
        if edf & 0x80 != 0 {
            game_port = reader.u16_le();
        }
        if edf & 0x10 != 0 {
            reader.bytes(8);
        }
        if edf & 0x40 != 0 {
            reader.u16_le();
            reader.cstring();
        }
        if edf & 0x20 != 0 {
            keywords = reader.cstring();
        }
    }

    Some(A2sInfo {
        ping_ms,
        name: Some(name),
        map: Some(map),
        players: Some(players),
        max_players: Some(max_players),
        version,
        game_port: game_port.filter(|&port| port != 0),
        password: Some(visibility != 0),
        keywords,
    })
}

/// How many datagrams we are willing to send before giving up. Covers the
/// challenge round trip plus ordinary UDP packet loss.
const MAX_ATTEMPTS: u32 = 4;
/// A server that keeps handing out challenges is not going to answer.
const MAX_CHALLENGES: u32 = 2;

/// A server that answered, but not with data we can read. It is online and its
/// latency is known, which is all the table needs.
fn alive_only(ping_ms: u32) -> A2sInfo {
    A2sInfo {
        ping_ms,
        name: None,
        map: None,
        players: None,
        max_players: None,
        version: None,
        game_port: None,
        password: None,
        keywords: None,
    }
}

/// A UDP socket connected to one server, so stray datagrams from anyone else
/// are dropped by the kernel.
async fn connect(ip: &str, query_port: u16) -> Result<UdpSocket, String> {
    let mut addrs = tokio::net::lookup_host((ip, query_port))
        .await
        .map_err(|e| format!("Address not resolvable: {e}"))?;
    let addr = addrs.next().ok_or("Address not resolvable")?;

    let bind = if addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let socket = UdpSocket::bind(bind)
        .await
        .map_err(|e| format!("Could not open UDP socket: {e}"))?;
    socket
        .connect(addr)
        .await
        .map_err(|e| format!("Could not connect the socket: {e}"))?;
    Ok(socket)
}

/// Queries one server, solving the challenge when required.
pub async fn query(ip: &str, query_port: u16, wait: Duration) -> Result<A2sInfo, String> {
    let socket = connect(ip, query_port).await?;

    let mut request = A2S_INFO_HEADER.to_vec();
    let mut buf = [0u8; 4096];
    // Latency is measured on the first round trip that comes back, which is
    // the one that reflects network distance; the challenge adds a second hop
    // and a retry adds a whole timeout, so neither is allowed to count.
    let mut ping_ms: Option<u32> = None;

    let mut attempts = 0;
    let mut challenges = 0;

    while attempts < MAX_ATTEMPTS {
        attempts += 1;

        let sent_at = Instant::now();
        socket
            .send(&request)
            .await
            .map_err(|e| format!("Could not send the query: {e}"))?;

        let received = match timeout(wait, socket.recv(&mut buf)).await {
            // A dropped datagram is ordinary on UDP. Retrying is the whole
            // point of having a budget of attempts; bailing out here is what
            // used to mark healthy servers as offline on every other refresh.
            Err(_) => continue,
            Ok(Err(e)) => return Err(format!("Receive error: {e}")),
            Ok(Ok(received)) => received,
        };

        let elapsed = sent_at.elapsed().as_millis().min(u32::MAX as u128) as u32;
        let ping = *ping_ms.get_or_insert(elapsed);

        if received < 5 {
            return Err("Reply too short".into());
        }

        let header = &buf[..4];
        if header == MULTI_PACKET {
            // Split response. Extremely rare for A2S_INFO, and reassembling it
            // buys us nothing here: the server answered, so it is online and
            // its latency is already measured. Reporting that beats throwing
            // the measurement away and calling it offline.
            return Ok(alive_only(ping));
        }
        if header != SINGLE_PACKET {
            return Err("Unknown header".into());
        }

        let payload = &buf[4..received];

        match payload.first() {
            // Challenge: repeat the query with the four bytes appended.
            Some(b'A') if payload.len() >= 5 => {
                challenges += 1;
                if challenges > MAX_CHALLENGES {
                    return Err("Server kept asking for a challenge".into());
                }
                request = A2S_INFO_HEADER.to_vec();
                request.extend_from_slice(&payload[1..5]);
            }
            Some(b'I') => {
                return Ok(parse_info(payload, ping).unwrap_or_else(|| alive_only(ping)));
            }
            _ => return Err("Unexpected reply".into()),
        }
    }

    Err("No reply (timed out)".into())
}

/// A rules reply split over more packets than this is not a DayZ server's.
/// DayZ needs three or four for a heavily modded server.
const MAX_RULE_PACKETS: u8 = 32;
const MAX_DATAGRAM: usize = 65_536;
const LOOKUP_DEADLINE: Duration = Duration::from_secs(8);

/// Fetches A2S_RULES as raw key/value pairs.
///
/// Unlike A2S_INFO, rules on a modded DayZ server routinely span several
/// datagrams, so split replies are reassembled here instead of waved through.
async fn rules(
    ip: &str,
    query_port: u16,
    wait: Duration,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
    let socket = connect(ip, query_port).await?;

    let mut request = A2S_RULES_HEADER.to_vec();
    request.extend_from_slice(&[0xFF; 4]);
    // Heavily modded servers answer with one oversized, IP-fragmented
    // datagram rather than a split reply: 4.3 KB for 98 mods. Anything that
    // does not fit is silently cut short, so take the largest a datagram can be.
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut attempts = 0;

    let payload = loop {
        attempts += 1;
        if attempts > MAX_ATTEMPTS {
            return Err("No reply to the rules query (timed out)".into());
        }
        socket
            .send(&request)
            .await
            .map_err(|e| format!("Could not send the query: {e}"))?;

        let received = match timeout(wait, socket.recv(&mut buf)).await {
            Err(_) => continue,
            Ok(Err(e)) => return Err(format!("Receive error: {e}")),
            Ok(Ok(received)) => received,
        };
        let packet = &buf[..received];

        if packet.starts_with(&SINGLE_PACKET) {
            match packet.get(4) {
                Some(b'A') if packet.len() >= 9 => {
                    request = A2S_RULES_HEADER.to_vec();
                    request.extend_from_slice(&packet[5..9]);
                }
                Some(b'E') => break packet[4..].to_vec(),
                _ => return Err("Unexpected reply to the rules query".into()),
            }
        } else if packet.starts_with(&MULTI_PACKET) {
            break reassemble(&socket, packet, wait).await?;
        } else {
            return Err("Unknown header".into());
        }
    };

    parse_rules(&payload).ok_or_else(|| "Unreadable rules reply".to_string())
}

/// Collects the rest of a split reply (Source layout: id, total, number,
/// size) and returns the payload after its own single-packet header.
async fn reassemble(socket: &UdpSocket, first: &[u8], wait: Duration) -> Result<Vec<u8>, String> {
    fn split(packet: &[u8]) -> Option<(u32, u8, u8, &[u8])> {
        let id = u32::from_le_bytes(packet.get(4..8)?.try_into().ok()?);
        Some((id, *packet.get(8)?, *packet.get(9)?, packet.get(12..)?))
    }

    let (id, total, _, _) = split(first).ok_or("Truncated split reply")?;
    if id & 0x8000_0000 != 0 {
        return Err("Compressed replies are not supported".into());
    }
    if total == 0 || total > MAX_RULE_PACKETS {
        return Err("Implausible split reply".into());
    }

    let mut parts: Vec<Option<Vec<u8>>> = vec![None; total as usize];
    let store = |parts: &mut Vec<Option<Vec<u8>>>, packet: &[u8]| {
        if let Some((pid, ptotal, number, data)) = split(packet) {
            if pid == id && ptotal == total && (number as usize) < parts.len() {
                parts[number as usize] = Some(data.to_vec());
            }
        }
    };
    store(&mut parts, first);

    let mut buf = vec![0u8; MAX_DATAGRAM];
    // Stray or repeated parts count too, so a server cannot keep this going.
    let mut budget = 2 * total as usize;
    while parts.iter().any(Option::is_none) {
        budget = budget
            .checked_sub(1)
            .ok_or("Too many packets in a split reply")?;
        let received = timeout(wait, socket.recv(&mut buf))
            .await
            .map_err(|_| "Part of the reply never arrived".to_string())?
            .map_err(|e| format!("Receive error: {e}"))?;
        if buf[..received].starts_with(&MULTI_PACKET) {
            store(&mut parts, &buf[..received]);
        }
    }

    let joined: Vec<u8> = parts.into_iter().flatten().flatten().collect();
    joined
        .strip_prefix(&SINGLE_PACKET[..])
        .map(<[u8]>::to_vec)
        .ok_or_else(|| "Malformed split reply".into())
}

/// Parses an A2S_RULES payload (starting at its `E`) into raw pairs. Keys are
/// kept as bytes: DayZ's mod list lives under two-byte binary keys.
fn parse_rules(payload: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut reader = Reader::new(payload);
    if reader.u8()? != b'E' {
        return None;
    }
    let count = reader.u16_le()?;
    let mut rules = Vec::new();
    for _ in 0..count {
        let key = raw_cstring(&mut reader)?;
        let value = raw_cstring(&mut reader)?;
        rules.push((key, value));
    }
    Some(rules)
}

fn raw_cstring(reader: &mut Reader) -> Option<Vec<u8>> {
    let rest = reader.buf.get(reader.pos..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    reader.pos += end + 1;
    Some(rest[..end].to_vec())
}

/// The mods a DayZ server announces about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct DayzMods {
    /// `(workshop id, name)`, in the server's load order.
    pub mods: Vec<(u64, String)>,
    /// The server had more mods than fit in a rules reply, so this list is
    /// missing some.
    pub truncated: bool,
}

/// Decodes the mod list DayZ hides in its A2S rules.
///
/// The server splits a binary blob into chunks stored under two-byte keys
/// (`[index, total]`, counting from one), escaping the bytes a C string
/// cannot carry: `01 01` is 0x01, `01 02` is 0x00 and `01 03` is 0xFF. The blob
/// is: protocol version, overflow flags, a DLC bitmask followed by one hash per
/// set bit, then a count and, per mod, a hash, an id length (low nibble), the
/// little-endian Workshop id and a length-prefixed name.
pub fn decode_dayz_mods(rules: &[(Vec<u8>, Vec<u8>)]) -> Option<DayzMods> {
    let mut chunks: Vec<(u8, &[u8])> = rules
        .iter()
        .filter(|(key, _)| key.len() == 2 && key[0] >= 1 && key[0] <= key[1])
        .map(|(key, value)| (key[0], value.as_slice()))
        .collect();
    if chunks.is_empty() {
        // A vanilla server has no blob at all, and that means no mods.
        return Some(DayzMods {
            mods: Vec::new(),
            truncated: false,
        });
    }
    chunks.sort_by_key(|(index, _)| *index);

    // Joined before unescaping: a chunk can end halfway through an escape,
    // which happens on most servers with a long mod list.
    let joined: Vec<u8> = chunks
        .into_iter()
        .flat_map(|(_, chunk)| chunk.iter().copied())
        .collect();
    let mut blob = Vec::with_capacity(joined.len());
    let mut bytes = joined.into_iter();
    while let Some(byte) = bytes.next() {
        if byte == 0x01 {
            blob.push(match bytes.next()? {
                0x01 => 0x01,
                0x02 => 0x00,
                0x03 => 0xFF,
                _ => return None,
            });
        } else {
            blob.push(byte);
        }
    }

    let mut reader = Reader::new(&blob);
    let _protocol = reader.u8()?;
    let overflow = reader.u8()?;
    let dlc = reader.u16_le()?;
    reader.bytes(4 * dlc.count_ones() as usize)?;

    let count = reader.u8()?;
    let mut mods = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let _hash = reader.bytes(4)?;
        let id_len = (reader.u8()? & 0x0F) as usize;
        if id_len > 8 {
            return None;
        }
        let mut id = [0u8; 8];
        id[..id_len].copy_from_slice(reader.bytes(id_len)?);
        let name_len = reader.u8()? as usize;
        let name = String::from_utf8_lossy(reader.bytes(name_len)?).into_owned();
        mods.push((u64::from_le_bytes(id), name));
    }

    Some(DayzMods {
        mods,
        truncated: overflow & 0x01 != 0,
    })
}

/// Asks the server itself for what dayzsalauncher.com would have said about
/// it, in the same shape. Returns whether the mod list came back truncated.
pub async fn lookup(ip: &str, query_port: u16) -> Result<(ApiServer, bool), String> {
    let wait = Duration::from_millis(1_500);
    // One deadline over the whole exchange: per-datagram timeouts alone let a
    // server that keeps trickling packets in hold the panel open forever.
    let (info, rules) = timeout(LOOKUP_DEADLINE, async {
        tokio::join!(query(ip, query_port, wait), rules(ip, query_port, wait))
    })
    .await
    .map_err(|_| "The server took too long to describe itself".to_string())?;
    let info = info?;
    let mods = decode_dayz_mods(&rules?)
        .ok_or_else(|| "The server's mod list could not be read".to_string())?;

    let keywords: Vec<&str> = info
        .keywords
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .collect();
    let has = |flag: &str| keywords.contains(&flag);
    // The in-game clock is the keyword shaped like `14:15`.
    let time = keywords
        .iter()
        .find(|k| k.len() == 5 && k.as_bytes()[2] == b':')
        .map(|k| k.to_string());

    let server = ApiServer {
        endpoint: Endpoint {
            ip: ip.to_string(),
            port: query_port,
        },
        game_port: info.game_port.unwrap_or(0),
        name: info.name.unwrap_or_default(),
        map: info.map.unwrap_or_default(),
        players: info.players.unwrap_or(0),
        max_players: info.max_players.unwrap_or(0),
        password: info.password.unwrap_or(false),
        version: info.version.unwrap_or_default(),
        first_person_only: has("no3rd"),
        battle_eye: has("battleye"),
        time,
        time_acceleration: None,
        shard: None,
        mods: mods
            .mods
            .into_iter()
            .map(|(id, name)| ModRef { id, name })
            .collect(),
    };

    if !server.is_usable() {
        return Err(format!("{ip}:{query_port} is not a usable address"));
    }
    Ok((server, mods.truncated))
}

/// Upper bound on one batch. The table shows 50 rows; anything beyond this is
/// a caller mistake, and each target holds a 4 KiB buffer while it waits.
pub const MAX_TARGETS: usize = 256;

/// Queries many servers at once. Meant for the visible page of the table, not
/// for all 21,000 servers in the list.
pub async fn ping_many(mut targets: Vec<(String, u16)>, wait_ms: u64) -> Vec<PingResult> {
    use tokio::sync::Semaphore;

    // The same server twice on a page is one probe, not two.
    let mut seen = std::collections::HashSet::new();
    targets.retain(|target| seen.insert(target.clone()));
    targets.truncate(MAX_TARGETS);

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
    fn reads_the_game_port_and_keywords_from_the_extra_data() {
        let mut reply = real_reply();
        reply.push(0x80 | 0x20 | 0x10 | 0x01); // port, keywords, steam id, game id
        reply.extend_from_slice(&2302u16.to_le_bytes());
        reply.extend_from_slice(&[0; 8]); // steam id
        reply.extend_from_slice(b"battleye,no3rd,privHive,mod,14:15\0");
        reply.extend_from_slice(&221100u64.to_le_bytes());

        let info = parse_info(&reply, 1).expect("should parse");
        assert_eq!(info.game_port, Some(2302));
        assert_eq!(info.password, Some(false));
        assert_eq!(
            info.keywords.as_deref(),
            Some("battleye,no3rd,privHive,mod,14:15")
        );
    }

    /// The blob layout captured from a live server (Mystery Nasdara), cut down
    /// to two mods, split over two chunks that arrive out of order.
    fn dayz_rules() -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut blob = vec![2, 0, 0x04, 0x00, 0xA2, 0xE1, 0x38, 0x50, 2];
        blob.extend_from_slice(&[0x4B, 0xAB, 0x88, 0x15, 4]);
        blob.extend_from_slice(&2464526692u32.to_le_bytes());
        blob.push(8);
        blob.extend_from_slice(b"GameLabs");
        // An id carrying 0x00, 0x01 and 0xFF bytes, which all need escaping.
        blob.extend_from_slice(&[0, 0, 0, 0, 4]);
        blob.extend_from_slice(&0xFF01_0001u32.to_le_bytes());
        blob.push(2);
        blob.extend_from_slice(b"CF");
        blob.extend_from_slice(&[1, 3, b'V', b'P', b'P']); // signatures

        let mut escaped = Vec::new();
        for byte in blob {
            match byte {
                0x01 => escaped.extend_from_slice(&[0x01, 0x01]),
                0x00 => escaped.extend_from_slice(&[0x01, 0x02]),
                0xFF => escaped.extend_from_slice(&[0x01, 0x03]),
                other => escaped.push(other),
            }
        }
        // Split right after an escape byte, as real servers do.
        let cut = escaped.iter().position(|&b| b == 0x01).unwrap() + 1;
        let (first, second) = escaped.split_at(cut);
        vec![
            (b"dedicated".to_vec(), b"1".to_vec()),
            (vec![2, 2], second.to_vec()),
            (vec![1, 2], first.to_vec()),
            (b"island".to_vec(), b"nasdara".to_vec()),
        ]
    }

    #[test]
    fn decodes_the_mod_list_dayz_hides_in_its_rules() {
        let decoded = decode_dayz_mods(&dayz_rules()).expect("should decode");
        assert_eq!(
            decoded.mods,
            vec![
                (2464526692, "GameLabs".to_string()),
                (0xFF01_0001, "CF".to_string()),
            ]
        );
        assert!(!decoded.truncated);
    }

    #[test]
    fn a_vanilla_server_has_no_mods_rather_than_an_unreadable_list() {
        let rules = vec![(b"island".to_vec(), b"chernarusplus".to_vec())];
        assert_eq!(decode_dayz_mods(&rules).unwrap().mods, vec![]);
    }

    #[test]
    fn a_damaged_mod_blob_is_refused_without_panicking() {
        let rules = dayz_rules();
        let (_, chunk) = &rules[2];
        for cut in 0..chunk.len() {
            let damaged = vec![(vec![1, 1], chunk[..cut].to_vec())];
            let _ = decode_dayz_mods(&damaged);
        }
        // A bogus escape is refused rather than guessed at.
        assert!(decode_dayz_mods(&[(vec![1, 1], vec![2, 0x01, 0x09])]).is_none());
    }

    #[test]
    fn parses_raw_rules_with_binary_keys() {
        let mut payload = vec![b'E', 2, 0];
        payload.extend_from_slice(b"\x01\x02\0abc\0island\0enoch\0");
        let rules = parse_rules(&payload).expect("should parse");
        assert_eq!(rules[0], (vec![1, 2], b"abc".to_vec()));
        assert_eq!(rules[1], (b"island".to_vec(), b"enoch".to_vec()));
        assert!(parse_rules(&payload[..payload.len() - 1]).is_none());
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
