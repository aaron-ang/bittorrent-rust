use std::mem;

use anyhow::{bail, Result};

#[derive(Debug)]
pub struct Message {
    length: u32,
    pub id: MessageId,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new(id: MessageId, payload: Vec<u8>) -> Self {
        let length = (mem::size_of::<MessageId>() + payload.len()) as u32;
        Self {
            length,
            id,
            payload,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let msg_len =
            mem::size_of_val(&self.length) + mem::size_of_val(&self.id) + self.payload.len();
        let mut bytes = Vec::with_capacity(msg_len);
        bytes.extend_from_slice(&self.length.to_be_bytes());
        bytes.push(self.id as u8);
        bytes.extend_from_slice(&self.payload);
        bytes
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum MessageId {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
    Reject = 16,
    Extension = 20,
}

impl TryFrom<u8> for MessageId {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Choke),
            1 => Ok(Self::Unchoke),
            2 => Ok(Self::Interested),
            3 => Ok(Self::NotInterested),
            4 => Ok(Self::Have),
            5 => Ok(Self::Bitfield),
            6 => Ok(Self::Request),
            7 => Ok(Self::Piece),
            8 => Ok(Self::Cancel),
            16 => Ok(Self::Reject),
            20 => Ok(Self::Extension),
            v => bail!("Invalid message id: {}", v),
        }
    }
}
