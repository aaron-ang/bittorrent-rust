#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::mem;

#[derive(Serialize, Deserialize)]
pub struct ExtensionHeader {
    pub m: ExtensionMetadata,
    p: Option<u16>, // port
    metadata_size: u32,
}

#[derive(Serialize, Deserialize)]
pub struct ExtensionMetadata {
    pub ut_metadata: u8,
    ut_pex: Option<u8>,
}

impl ExtensionHeader {
    pub fn new() -> Self {
        let metadata = ExtensionMetadata {
            ut_metadata: 1,
            ut_pex: Some(2),
        };
        let port = Some(6881);
        let size = (mem::size_of_val(&metadata) + mem::size_of_val(&port)) as u32;

        Self {
            m: metadata,
            p: port,
            metadata_size: size,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ExtensionMessage {
    pub msg_type: ExtensionMessageType,
    pub piece: u8,
    pub total_size: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub enum ExtensionMessageType {
    Request,
    Data,
    Reject,
}
