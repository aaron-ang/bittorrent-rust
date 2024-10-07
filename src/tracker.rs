use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[derive(Serialize)]
pub struct TrackerRequest {
    pub peer_id: String,
    pub port: u16,
    pub uploaded: usize,
    pub downloaded: usize,
    pub left: u32,
    pub compact: u8,
}

impl TrackerRequest {
    pub fn new(left: u32) -> Self {
        Self {
            peer_id: String::from("00112233445566778899"),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left,
            compact: 1,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct TrackerResponse {
    interval: Option<u32>,
    #[serde(with = "serde_bytes")]
    peers: Vec<u8>,
}

impl TrackerResponse {
    pub fn peers(&self) -> Vec<SocketAddr> {
        self.peers
            .chunks_exact(6)
            .map(|chunk| {
                let ip = IpAddr::V4(Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]));
                let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                SocketAddr::new(ip, port)
            })
            .collect()
    }
}
