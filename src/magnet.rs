use std::collections::HashMap;
use url::Url;

const MAGNET_XT_PREFIX: &'static str = "urn:btih:";

pub struct Magnet {
    pub info_hash: [u8; 20], // raw bytes
    pub file_name: Option<String>,
    pub tracker_url: Option<Url>,
}

impl Magnet {
    pub fn new(link: &String) -> anyhow::Result<Self> {
        let url = Url::parse(link)?;
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
}
