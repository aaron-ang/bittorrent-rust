use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{net::SocketAddrV4, path::PathBuf};
use url::form_urlencoded;

use crate::{
    peer::Peer,
    tracker::{TrackerRequest, TrackerResponse},
};

#[derive(Serialize, Deserialize)]
pub struct Torrent {
    pub announce: String,
    pub info: Info,
}

impl Torrent {
    pub fn new(file_name: PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read(file_name)?;
        Ok(serde_bencode::from_bytes::<Self>(&content)?)
    }

    pub fn info_hash(&self) -> anyhow::Result<[u8; 20]> {
        Ok(Sha1::digest(serde_bencode::to_bytes(&self.info)?).into())
    }

    pub fn pieces(&self) -> Vec<&[u8]> {
        self.info.pieces.chunks_exact(20).collect()
    }

    pub async fn get_peer_addrs(&self) -> anyhow::Result<Vec<SocketAddrV4>> {
        let info_hash_str: String = form_urlencoded::byte_serialize(&self.info_hash()?).collect();
        let request = TrackerRequest {
            peer_id: String::from("00112233445566778899"),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left: self.info.length,
            compact: 1,
        };
        let params = serde_urlencoded::to_string(&request)?;
        let url = format!("{}?{}&info_hash={}", self.announce, params, info_hash_str);
        let response = reqwest::get(url).await?;
        let tracker_response =
            serde_bencode::from_bytes::<TrackerResponse>(&response.bytes().await?)?;
        Ok(tracker_response.peers())
    }

    pub async fn find_peer_with_piece(&self, piece: usize) -> anyhow::Result<Peer> {
        let info_hash = self.info_hash()?;
        for peer_address in self.get_peer_addrs().await? {
            match Peer::handshake(peer_address, info_hash).await {
                Ok(mut peer) => {
                    let pieces = peer.get_pieces().await?;
                    if pieces.contains(&piece) {
                        return Ok(peer);
                    }
                }
                Err(e) => eprintln!("{} -> {}", peer_address, e),
            }
        }
        Err(anyhow::anyhow!("Could not find peer"))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Info {
    pub length: u32,
    name: String,
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
}
