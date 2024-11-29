use std::{collections::HashMap, net::SocketAddr};

use anyhow::Result;
use url::Url;

use crate::{peer::Peer, torrent::Torrent};

const MAGNET_XT_PREFIX: &'static str = "urn:btih:";

pub struct Magnet {
    pub info_hash: [u8; 20], // raw bytes
    pub file_name: Option<String>,
    pub tracker_url: Option<Url>,
}

impl Magnet {
    pub fn new(url: Url) -> Result<Self> {
        if url.scheme() != "magnet" {
            anyhow::bail!("invalid magnet link");
        }

        let query_pairs: HashMap<_, _> = url.query_pairs().collect();
        let xt = query_pairs
            .get("xt")
            .ok_or_else(|| anyhow::anyhow!("missing xt"))?;

        if !xt.starts_with(MAGNET_XT_PREFIX) {
            anyhow::bail!("invalid xt");
        }

        let info_hash = hex::decode(&xt[MAGNET_XT_PREFIX.len()..])?
            .try_into()
            .map_err(|_| anyhow::anyhow!("info hash must be 20 bytes"))?;
        let file_name = query_pairs.get("dn").map(|s| s.to_string());
        let tracker_url = query_pairs.get("tr").map(|s| Url::parse(s)).transpose()?;

        Ok(Self {
            info_hash,
            file_name,
            tracker_url,
        })
    }

    pub async fn handshake(&self) -> Result<Peer> {
        for peer_address in self.fetch_peer_addresses().await? {
            match Peer::new(peer_address, self.info_hash).await {
                Ok(mut peer) => {
                    if peer.supports_extension {
                        peer.extension_handshake().await?;
                        return Ok(peer);
                    }
                }
                Err(e) => eprintln!("{} -> {}", peer_address, e),
            }
        }
        anyhow::bail!("Could not find peer with extension support")
    }

    async fn fetch_peer_addresses(&self) -> Result<Vec<SocketAddr>> {
        let mut torrent = Torrent::from_magnet(self)?;
        torrent.fetch_peer_addresses().await
    }

    pub async fn download_piece(&self, piece: usize) -> Result<Vec<u8>> {
        let mut torrent = Torrent::from_magnet(self)?;
        torrent.download_piece(piece).await
    }

    pub async fn download(&self) -> Result<Vec<u8>> {
        let mut torrent = Torrent::from_magnet(self)?;
        torrent.download().await
    }
}
