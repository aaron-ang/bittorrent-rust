use anyhow::Context;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use tokio::{net::UdpSocket, task::JoinSet};
use url::form_urlencoded;

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

#[derive(Clone, Serialize, Deserialize)]
struct File {
    length: u32,
    path: Vec<String>,
}

impl Torrent {
    pub fn new(file_name: PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read(file_name)?;
        Ok(serde_bencode::from_bytes::<Self>(&content)?)
    }

    pub fn from_magnet_and_metadata(magnet: Magnet, metadata: Info) -> anyhow::Result<Self> {
        Ok(Self {
            announce: magnet.tracker_url.unwrap().to_string(),
            info: metadata,
        })
    }

    pub fn info_hash(&self) -> anyhow::Result<[u8; 20]> {
        Ok(Sha1::digest(serde_bencode::to_bytes(&self.info)?).into())
    }

    pub fn len(&self) -> u32 {
        match &self.info.additional {
            Additional::SingleFile { length } => *length,
            Additional::MultiFile { files } => files.iter().map(|f| f.length).sum(),
        }
    }

    pub fn pieces(&self) -> Vec<Vec<u8>> {
        self.info
            .pieces
            .chunks_exact(20)
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    pub async fn get_peer_addrs(&self) -> anyhow::Result<Vec<SocketAddr>> {
        let info_hash_str: String = form_urlencoded::byte_serialize(&self.info_hash()?).collect();
        let request = TrackerRequest::new(self.len());
        let announce = &self.announce;
        if announce.starts_with("http") {
            let params = serde_urlencoded::to_string(&request)?;
            let url = format!("{}?{}&info_hash={}", announce, params, info_hash_str);
            let response = reqwest::get(url).await?;
            let tracker_response =
                serde_bencode::from_bytes::<TrackerResponse>(&response.bytes().await?)?;
            let peer_addrs = tracker_response.peers();
            println!("Found peers: {:?}", peer_addrs);
            Ok(peer_addrs)
        } else if announce.starts_with("udp") {
            let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
            let address = Self::parse_udp_url(announce)?;
            sock.connect(address).await?;
            Ok(vec![])
        } else {
            Err(anyhow::anyhow!("Unsupported tracker protocol"))
        }
    }

    fn parse_udp_url(url: &str) -> anyhow::Result<String> {
        let parts: Vec<&str> = url.split(':').collect();
        let host = parts[1].trim_start_matches('/');
        let port = parts[2];
        let addr = format!("{}:{}", host, port);
        Ok(addr)
    }

    pub async fn download(&self) -> anyhow::Result<Vec<u8>> {
        let peer_addrs = self.get_peer_addrs().await?;
        let piece_hashes = self.pieces();
        let num_pieces = piece_hashes.len();
        let info_hash = self.info_hash()?;
        let mut peer_piece_map: HashMap<usize, Vec<Peer>> = HashMap::new();
        let mut join_set = JoinSet::new();

        for peer_address in peer_addrs {
            match Peer::new(peer_address, info_hash).await {
                Ok(mut peer) => {
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
            return Err(anyhow::anyhow!("Could not connect to any peers"));
        }

        let choose_peer = |piece: usize| {
            let peers = peer_piece_map.get(&piece).unwrap();
            peers.choose(&mut rand::thread_rng()).unwrap().clone()
        };

        let spawn = |join_set: &mut JoinSet<_>, piece: usize| {
            let mut peer = choose_peer(piece);
            let torrent = self.clone();
            let piece_hashes = piece_hashes.clone();
            let piece_number = piece + 1;
            let piece_len = std::cmp::min(
                torrent.info.piece_length,
                torrent.len() - piece as u32 * torrent.info.piece_length,
            );

            join_set.spawn(async move {
                match peer.load_piece(piece as u32, piece_len).await {
                    Ok(data) => {
                        println!(
                            "Downloaded piece {}/{} from peer {}",
                            piece_number, num_pieces, peer.address
                        );
                        if piece_hashes[piece] != *Sha1::digest(&data) {
                            eprintln!(
                                "Piece {}/{} failed verification. Will retry...",
                                piece_number, num_pieces
                            );
                            (piece, vec![])
                        } else {
                            (piece, data)
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Error loading piece {}/{}: {}. Will retry...",
                            piece_number, num_pieces, e
                        );
                        (piece, vec![])
                    }
                }
            });
        };

        for piece in 0..num_pieces {
            spawn(&mut join_set, piece);
        }

        let mut file_bytes = vec![0u8; self.len() as usize];
        let piece_len = self.info.piece_length as usize;
        while let Some(join_result) = join_set.join_next().await {
            let (piece, data) = join_result.context("Task panicked")?;
            if data.is_empty() {
                spawn(&mut join_set, piece);
            } else {
                let start = piece * piece_len;
                let end = start + data.len();
                file_bytes[start..end].copy_from_slice(&data);
            }
        }

        Ok(file_bytes)
    }
}
