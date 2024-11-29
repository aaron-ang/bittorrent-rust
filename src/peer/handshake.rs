use serde::{Deserialize, Serialize};

use crate::peer::Peer;

const PROTOCOL: &str = "BitTorrent protocol";
const PROTOCOL_LEN: usize = PROTOCOL.len();
const PEER_ID_LEN: usize = 20;
const EXTENSION_SUPPORT_FLAG: u64 = 1 << 20;

#[derive(Serialize, Deserialize)]
pub struct Handshake {
    pub length: u8,
    pub protocol: [u8; PROTOCOL_LEN],
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; PEER_ID_LEN],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20]) -> Self {
        let mut reserved = 0;
        reserved |= EXTENSION_SUPPORT_FLAG;
        let peer_id: [u8; 20] = Peer::gen_peer_id().as_bytes().try_into().unwrap();
        Self {
            length: PROTOCOL_LEN as u8,
            protocol: PROTOCOL.as_bytes().try_into().unwrap(),
            reserved: reserved.to_be_bytes(),
            info_hash,
            peer_id,
        }
    }

    pub fn supports_extension(&self) -> bool {
        self.reserved[5] & 0x10 != 0
    }
}
