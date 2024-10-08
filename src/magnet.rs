use std::{collections::HashMap, net::SocketAddr};
use url::{form_urlencoded, Url};

use crate::{
    peer::Peer,
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
}
