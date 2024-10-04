use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;
use url::form_urlencoded;

#[derive(Serialize, Deserialize)]
struct Torrent {
    announce: String,
    info: Info,
}

impl Torrent {
    pub fn info_hash(&self) -> [u8; 20] {
        Sha1::digest(serde_bencode::to_bytes(&self.info).unwrap()).into()
    }
}

#[derive(Serialize, Deserialize)]
struct Info {
    length: usize,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: usize,
    #[serde(with = "serde_bytes")]
    pieces: Vec<u8>,
}

#[derive(Serialize)]
struct TrackerRequest {
    peer_id: String,
    port: u16,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    compact: u8,
}

#[derive(Serialize, Deserialize)]
struct Tracker {
    interval: usize,
    #[serde(with = "serde_bytes")]
    peers: Vec<u8>,
}

impl Tracker {
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

pub fn parse_torrent_file(file_name: PathBuf) -> anyhow::Result<()> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    println!("Tracker URL: {}", torrent.announce);
    println!("Length: {}", torrent.info.length);
    println!("Info Hash: {}", hex::encode(torrent.info_hash()));
    println!("Piece Length: {}", torrent.info.piece_length);
    println!("Piece Hashes:");
    for piece in torrent.info.pieces.chunks_exact(20) {
        println!("{}", hex::encode(piece));
    }
    Ok(())
}

pub fn discover_peers(file_name: PathBuf) -> anyhow::Result<()> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    let info_hash_str = form_urlencoded::byte_serialize(&torrent.info_hash()).collect::<String>();
    let request = TrackerRequest {
        peer_id: String::from("00112233445566778899"),
        port: 6881,
        uploaded: 0,
        downloaded: 0,
        left: torrent.info.length,
        compact: 1,
    };
    let params = serde_urlencoded::to_string(&request)?;
    let url = format!(
        "{}?{}&info_hash={}",
        torrent.announce, params, info_hash_str
    );
    let response = reqwest::blocking::get(url)?;
    let tracker = serde_bencode::from_bytes::<Tracker>(&response.bytes()?)?;
    for peer in tracker.peers() {
        println!("{}", peer);
    }
    Ok(())
}
