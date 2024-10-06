use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Serialize)]
pub struct TrackerRequest {
    pub peer_id: String,
    pub port: u16,
    pub uploaded: usize,
    pub downloaded: usize,
    pub left: u32,
    pub compact: u8,
}

#[derive(Serialize, Deserialize)]
pub struct TrackerResponse {
    interval: usize,
    #[serde(with = "serde_bytes")]
    peers: Vec<u8>,
}

impl TrackerResponse {
    pub fn peers(&self) -> Vec<SocketAddrV4> {
        self.peers
            .chunks_exact(6)
            .map(|chunk| {
                let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                SocketAddrV4::new(ip, port)
            })
            .collect()
    }
}
