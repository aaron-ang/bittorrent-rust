use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

#[derive(Serialize, Deserialize)]
pub struct Torrent {
    pub announce: String,
    pub info: Info,
}

impl Torrent {
    pub fn info_hash(&self) -> [u8; 20] {
        Sha1::digest(serde_bencode::to_bytes(&self.info).unwrap()).into()
    }
}

#[derive(Serialize, Deserialize)]
pub struct Info {
    pub length: usize,
    name: String,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
}
