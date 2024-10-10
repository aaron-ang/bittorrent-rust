use anyhow::Context;
use rand::seq::SliceRandom;
use sha1::{Digest, Sha1};
use std::{collections::HashMap, net::SocketAddr};
use tokio::task::JoinSet;
use url::{form_urlencoded, Url};

use crate::{
    peer::Peer,
    torrent::Info,
    tracker::{TrackerRequest, TrackerResponse},
};

const MAGNET_XT_PREFIX: &'static str = "urn:btih:";

pub struct Magnet {
    pub info_hash: [u8; 20], // raw bytes
    pub file_name: Option<String>,
    pub tracker_url: Option<Url>,
}

impl Magnet {
    pub fn new(url: Url) -> anyhow::Result<Self> {
        if url.scheme() != "magnet" {
            return Err(anyhow::anyhow!("invalid magnet link"));
        }

        let query_pairs = url.query_pairs().collect::<HashMap<_, _>>();
        let xt = query_pairs.get("xt").ok_or(anyhow::anyhow!("missing xt"))?;
        if !xt.starts_with(MAGNET_XT_PREFIX) {
            return Err(anyhow::anyhow!("invalid xt"));
        }

        let info_hash = hex::decode(&xt[MAGNET_XT_PREFIX.len()..])?
            .try_into()
            .map_err(|_| anyhow::anyhow!("info hash must be 20 bytes"))?;
        let file_name = query_pairs.get("dn").map(|s| s.to_string());
        let tracker_url = query_pairs.get("tr").map(|s| Url::parse(s)).transpose()?;

        let magnet = Self {
            info_hash,
            file_name,
            tracker_url,
        };
        Ok(magnet)
    }

    pub async fn get_peer_addrs(&self) -> anyhow::Result<Vec<SocketAddr>> {
        let request = TrackerRequest::new(1);
        let params = serde_urlencoded::to_string(&request)?;
        let info_hash_str: String = form_urlencoded::byte_serialize(&self.info_hash).collect();
        let url = format!(
            "{}?{}&info_hash={}",
            self.tracker_url.as_ref().unwrap(),
            params,
            info_hash_str,
        );

        let response = reqwest::get(url).await?;
        let tracker_response =
            serde_bencode::from_bytes::<TrackerResponse>(&response.bytes().await?)?;
        let peer_addrs = tracker_response.peers();
        println!("Found peers: {:?}", peer_addrs);
        Ok(peer_addrs)
    }

    pub async fn handshake(&self) -> anyhow::Result<Peer> {
        let peer_addrs = self.get_peer_addrs().await?;
        for peer_address in peer_addrs {
            match Peer::new(peer_address, self.info_hash).await {
                Ok(mut peer) => {
                    if peer.supports_extension {
                        peer.get_pieces().await?;
                        peer.extension_handshake().await?;
                    }
                    return Ok(peer);
                }
                Err(e) => eprintln!("{} -> {}", peer_address, e),
            }
        }
        Err(anyhow::anyhow!("Could not find peer"))
    }

    pub async fn download(&self) -> anyhow::Result<Vec<u8>> {
        let peer_addrs = self.get_peer_addrs().await?;
        let mut metadata: Option<Info> = None;
        let mut peer_piece_map: HashMap<usize, Vec<Peer>> = HashMap::new();
        let mut join_set = JoinSet::new();

        for peer_address in peer_addrs {
            match Peer::new(peer_address, self.info_hash).await {
                Ok(mut peer) => {
                    if peer.supports_extension {
                        let pieces = peer.get_pieces().await?;
                        peer.extension_handshake().await?;
                        if metadata.is_none() {
                            metadata = Some(peer.extension_metadata().await?);
                        }
                        for piece in pieces {
                            peer_piece_map
                                .entry(piece)
                                .or_insert_with(Vec::new)
                                .push(peer.clone());
                        }
                        peer.prepare_download().await?;
                    }
                }
                Err(e) => eprintln!("{} -> {}", peer_address, e),
            }
        }

        if peer_piece_map.is_empty() || metadata.is_none() {
            return Err(anyhow::anyhow!("Could not connect to any peers"));
        }

        let metadata = metadata.unwrap();
        let piece_hashes = metadata.pieces();
        let num_pieces = piece_hashes.len();
        let piece_len = metadata.piece_length;
        let file_len = metadata.file_len();

        let choose_peer = |piece: usize| {
            let peers = peer_piece_map.get(&piece).unwrap();
            peers.choose(&mut rand::thread_rng()).unwrap().clone()
        };

        let spawn = |join_set: &mut JoinSet<_>, piece: usize| {
            let mut peer = choose_peer(piece);
            let piece_hashes = piece_hashes.clone();
            let piece_number = piece + 1;
            let piece_len = std::cmp::min(piece_len, file_len - piece as u32 * piece_len);

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

        let mut file_bytes = vec![0u8; file_len as usize];
        while let Some(join_result) = join_set.join_next().await {
            let (piece, data) = join_result.context("Task panicked")?;
            if data.is_empty() {
                spawn(&mut join_set, piece);
            } else {
                let start = piece * piece_len as usize;
                let end = start + data.len();
                file_bytes[start..end].copy_from_slice(&data);
            }
        }
        Ok(file_bytes)
    }
}
