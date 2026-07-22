use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DzsaModInfo {
    #[serde(rename = "steamWorkshopId")]
    pub workshop_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DzsaQueryResult {
    pub name: Option<String>,
    pub map: Option<String>,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub mods: Option<Vec<DzsaModInfo>>,
    pub is_modded: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DzsaApiResponse {
    pub status: u32,
    pub result: Option<DzsaQueryResult>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerPingResult {
    pub ip: String,
    pub port: u16,
    pub ping_ms: u32,
    pub online: bool,
    pub name: Option<String>,
    pub map: Option<String>,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
}

/// Query DZSA Launcher API for server mod details (`https://dayzsalauncher.com/api/v1/query/<IP>/<PORT>`)
pub async fn query_dzsa_server(ip: &str, query_port: u16) -> Result<DzsaApiResponse, String> {
    let url = format!("https://dayzsalauncher.com/api/v1/query/{}/{}", ip, query_port);
    let client = reqwest::Client::builder()
        .user_agent("ApexDZ-Linux-Launcher/1.0")
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("API returned status {}", res.status()));
    }

    let json = res.json::<DzsaApiResponse>().await.map_err(|e| e.to_string())?;
    Ok(json)
}

/// Perform Valve A2S_INFO UDP query to measure server ping and basic state
pub async fn ping_server_a2s(ip: &str, query_port: u16) -> ServerPingResult {
    let addr_str = format!("{}:{}", ip, query_port);
    let addr: SocketAddr = match addr_str.parse() {
        Ok(a) => a,
        Err(_) => {
            return ServerPingResult {
                ip: ip.to_string(),
                port: query_port,
                ping_ms: 999,
                online: false,
                name: None,
                map: None,
                players: None,
                max_players: None,
            }
        }
    };

    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => {
            return ServerPingResult {
                ip: ip.to_string(),
                port: query_port,
                ping_ms: 999,
                online: false,
                name: None,
                map: None,
                players: None,
                max_players: None,
            }
        }
    };

    // Valve A2S_INFO payload: 0xFF 0xFF 0xFF 0xFF 'T' "Source Engine Query\0"
    let mut payload = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x54];
    payload.extend_from_slice(b"Source Engine Query\0");

    let start = Instant::now();
    let send_res = socket.send_to(&payload, addr).await;

    if send_res.is_err() {
        return ServerPingResult {
            ip: ip.to_string(),
            port: query_port,
            ping_ms: 999,
            online: false,
            name: None,
            map: None,
            players: None,
            max_players: None,
        };
    }

    let mut buf = [0u8; 1400];
    let recv_res = timeout(Duration::from_millis(1500), socket.recv_from(&mut buf)).await;

    let elapsed = start.elapsed().as_millis() as u32;

    match recv_res {
        Ok(Ok((len, _))) if len > 6 => {
            let mut name = None;
            let mut map = None;
            let mut players = None;
            let mut max_players = None;

            // Simple parse of A2S_INFO header
            if buf[0..4] == [0xFF, 0xFF, 0xFF, 0xFF] && buf[4] == 0x49 { // 'I'
                let slice = &buf[6..len];
                let parts: Vec<&[u8]> = slice.split(|&b| b == 0).collect();
                if parts.len() >= 3 {
                    name = String::from_utf8(parts[0].to_vec()).ok();
                    map = String::from_utf8(parts[1].to_vec()).ok();
                }
                if parts.len() >= 4 && parts[3].len() >= 3 {
                    let info_bytes = parts[3];
                    players = Some(info_bytes[0] as u32);
                    max_players = Some(info_bytes[1] as u32);
                }
            }

            ServerPingResult {
                ip: ip.to_string(),
                port: query_port,
                ping_ms: elapsed,
                online: true,
                name,
                map,
                players,
                max_players,
            }
        }
        _ => ServerPingResult {
            ip: ip.to_string(),
            port: query_port,
            ping_ms: 999,
            online: false,
            name: None,
            map: None,
            players: None,
            max_players: None,
        },
    }
}
