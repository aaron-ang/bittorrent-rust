use anyhow::Result;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use tokio::{net::UdpSocket, task::JoinSet};
use url::Url;

use crate::{
    magnet::Magnet,
    peer::Peer,
    tracker::{TrackerRequest, TrackerResponse},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub announce: String,
    pub info: Info,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Info {
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
    name: String,
    #[serde(flatten)]
    additional: Additional,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Additional {
    SingleFile { length: u32 },
    MultiFile { files: Vec<File> },
}

impl Info {
    pub fn pieces(&self) -> Vec<Vec<u8>> {
        self.pieces.chunks(20).map(|chunk| chunk.to_vec()).collect()
    }

    pub fn file_len(&self) -> u32 {
        match &self.additional {
            Additional::SingleFile { length } => *length,
            Additional::MultiFile { files } => files.iter().map(|file| file.length).sum(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct File {
    length: u32,
    path: Vec<String>,
}

impl Torrent {
    pub fn new(file_name: PathBuf) -> Result<Self> {
        let content = std::fs::read(file_name)?;
        let torrent = serde_bencode::from_bytes::<Self>(&content)?;
        Ok(torrent)
    }

    pub fn from_magnet_and_metadata(magnet: Magnet, metadata: Info) -> Result<Self> {
        Ok(Self {
            announce: magnet
                .tracker_url
                .map(|url| url.to_string())
                .unwrap_or_default(),
            info: metadata,
        })
    }

    pub fn info_hash(&self) -> Result<[u8; 20]> {
        let info_bytes = serde_bencode::to_bytes(&self.info)?;
        Ok(Sha1::digest(&info_bytes).into())
    }

    pub fn len(&self) -> u32 {
        self.info.file_len()
    }

    pub fn pieces(&self) -> Vec<Vec<u8>> {
        self.info.pieces()
    }

    pub async fn get_peer_addrs(&self) -> Result<Vec<SocketAddr>> {
        let info_hash = self.info_hash()?;
        let info_hash_str: String = url::form_urlencoded::byte_serialize(&info_hash).collect();
        let request = TrackerRequest::new(self.len());
        let mut announce_url = Url::parse(&self.announce)?;

        match announce_url.scheme() {
            "http" | "https" => {
                let params = serde_urlencoded::to_string(&request)?;
                announce_url.set_query(Some(&format!("{}&info_hash={}", params, info_hash_str)));
                let response = reqwest::get(announce_url).await?;
                let tracker_response =
                    serde_bencode::from_bytes::<TrackerResponse>(&response.bytes().await?)?;
                let peer_addrs = tracker_response.peers();
                println!("Found peers: {:?}", peer_addrs);
                Ok(peer_addrs)
            }
            "udp" => {
                let addr = format!(
                    "{}:{}",
                    announce_url.host_str().unwrap_or(""),
                    announce_url.port_or_known_default().unwrap_or(80)
                );
                let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
                sock.connect(addr).await?;
                todo!()
            }
            scheme => anyhow::bail!("Unsupported tracker protocol: {}", scheme),
        }
    }

    pub async fn download_piece(&self, piece_index: usize) -> Result<Vec<u8>> {
        let peer_addrs = self.get_peer_addrs().await?;
        let info_hash = self.info_hash()?;

        for peer_address in peer_addrs {
            match Peer::new(peer_address, info_hash).await {
                Ok(peer) => {
                    let pieces = peer.get_pieces().await?;
                    if pieces.contains(&piece_index) {
                        let piece_len = std::cmp::min(
                            self.info.piece_length,                                     // piece_len
                            self.len() - (piece_index as u32 * self.info.piece_length), // last piece
                        );
                        peer.prepare_download().await?;
                        return peer.load_piece(piece_index as u32, piece_len).await;
                    }
                }
                Err(e) => eprintln!("{} -> {}", peer_address, e),
            }
        }
        anyhow::bail!("Could not find peer with the requested piece")
    }

    pub async fn download(&self) -> Result<Vec<u8>> {
        // TODO:
        // 1. Add maximum retry limits
        // 2. Implement exponential backoff
        // 3. Add timeout mechanisms
        // 4. Consider switching to different peers after X failed attempts

        let peer_addrs = self.get_peer_addrs().await?;
        let piece_hashes = self.pieces();
        let num_pieces = piece_hashes.len();
        let info_hash = self.info_hash()?;
        let piece_len = self.info.piece_length;
        let file_len = self.len();

        let mut peer_piece_map: HashMap<usize, Vec<Peer>> = HashMap::new();
        let mut join_set = JoinSet::new();

        for peer_address in peer_addrs {
            match Peer::new(peer_address, info_hash).await {
                Ok(peer) => {
                    let pieces = peer.get_pieces().await?;
                    for piece in pieces {
                        peer_piece_map
                            .entry(piece)
                            .or_insert_with(Vec::new)
                            .push(peer.clone());
                    }
                    peer.prepare_download().await?;
                }
                Err(e) => eprintln!("{} -> {}", peer_address, e),
            }
        }

        if peer_piece_map.is_empty() {
            anyhow::bail!("Could not connect to any peers");
        }

        for piece in 0..num_pieces {
            if let Some(peers) = peer_piece_map.get(&piece) {
                let peer = peers.choose(&mut rand::thread_rng()).unwrap().clone();
                let piece_len = std::cmp::min(piece_len, file_len - (piece as u32 * piece_len));
                let piece_hash = piece_hashes[piece].clone();

                join_set.spawn(async move {
                    loop {
                        match peer.load_piece(piece as u32, piece_len).await {
                            Ok(piece_data) => {
                                let hash = Sha1::digest(&piece_data);
                                if hash.as_slice() == piece_hash.as_slice() {
                                    return (piece, piece_data);
                                } else {
                                    eprintln!(
                                        "Piece {} failed verification. Retrying...",
                                        piece + 1
                                    );
                                }
                            }
                            Err(err) => {
                                eprintln!(
                                    "Error loading piece {}: {}. Retrying...",
                                    piece + 1,
                                    err
                                );
                            }
                        }
                    }
                });
            }
        }

        let mut file_bytes = vec![0u8; file_len as usize];

        while let Some(join_result) = join_set.join_next().await {
            let (piece, data) = join_result?;
            let start = piece * piece_len as usize;
            let end = start + data.len();
            file_bytes[start..end].copy_from_slice(&data);
        }

        Ok(file_bytes)
    }
}
