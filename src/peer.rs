use anyhow::Context;
use bitvec::prelude::*;
use serde::{Deserialize, Serialize};
use std::{mem, net::SocketAddrV4, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    task::JoinSet,
    time::sleep,
};

use crate::torrent::Torrent;

const BLOCK_SIZE: u32 = 16 * 1024; // 16 KiB

#[derive(Serialize, Deserialize)]
pub struct Handshake {
    pub length: u8,
    pub protocol: [u8; 19],
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        Self {
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved: [0; 8],
            info_hash,
            peer_id,
        }
    }
}

#[derive(Clone)]
pub struct Peer {
    pub address: SocketAddrV4,
    pub id: [u8; 20],
    pub stream: Arc<Mutex<TcpStream>>,
}

impl Peer {
    pub async fn handshake(address: SocketAddrV4, info_hash: [u8; 20]) -> anyhow::Result<Self> {
        let mut handshake = Handshake::new(info_hash, *b"00112233445566778899");
        let mut handshake_bytes = bincode::serialize(&handshake)?;

        let mut peer_stream = TcpStream::connect(address)
            .await
            .context("failed to connect to peer")?;
        peer_stream
            .write_all(&handshake_bytes)
            .await
            .context("failed to send handshake")?;
        peer_stream
            .read_exact(&mut handshake_bytes)
            .await
            .context("failed to receive handshake")?;

        handshake = bincode::deserialize(&handshake_bytes)?;
        let peer = Peer {
            address,
            id: handshake.peer_id,
            stream: Arc::new(Mutex::new(peer_stream)),
        };
        Ok(peer)
    }

    async fn recv(&mut self) -> anyhow::Result<Message> {
        let mut stream = self.stream.lock().await;
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await?;
        let length = u32::from_be_bytes(buf);

        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf).await?;
        let id: MessageTag = unsafe { mem::transmute(buf[0]) };

        let mut buf = vec![0u8; length as usize - mem::size_of::<MessageTag>()];
        stream.read_exact(&mut buf).await?;
        Ok(Message {
            length,
            id,
            payload: buf,
        })
    }

    async fn send(&mut self, msg: Message) -> anyhow::Result<()> {
        let mut stream = self.stream.lock().await;
        stream.write_all(&msg.as_bytes()).await?;
        Ok(())
    }

    pub async fn get_pieces(&mut self) -> anyhow::Result<Vec<usize>> {
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::BITFIELD);
        let bitfield = BitVec::<u8, Msb0>::from_vec(msg.payload);
        let pieces = bitfield.iter_ones().collect();
        Ok(pieces)
    }

    pub async fn load_piece(&mut self, torrent: &Torrent, index: u32) -> anyhow::Result<Vec<u8>> {
        let interested = Message::new(MessageTag::INTERESTED, vec![]);
        self.send(interested).await?;
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::UNCHOKE);

        let piece_len = std::cmp::min(
            torrent.info.piece_length,                               // piece_len
            torrent.info.length - index * torrent.info.piece_length, // last piece
        );

        let mut piece = vec![0u8; piece_len as usize];
        let mut join_set = JoinSet::new();

        for offset in (0..piece_len).step_by(BLOCK_SIZE as usize) {
            let peer = self.clone();
            let length = BLOCK_SIZE.min(piece_len - offset);
            join_set.spawn(Self::load_block_with_retry(peer, index, offset, length));
        }

        while let Some(result) = join_set.join_next().await {
            let (offset, data) = result
                .context("Task panicked")?
                .context("Failed to load block")?;

            let start = offset as usize;
            let end = start + data.len();
            piece[start..end].copy_from_slice(&data);
        }

        Ok(piece)
    }

    async fn load_block_with_retry(
        mut peer: Peer,
        index: u32,
        offset: u32,
        length: u32,
    ) -> anyhow::Result<(u32, Vec<u8>)> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_secs(1);

        for attempt in 1..=MAX_RETRIES {
            match peer.load_block(index, offset, length).await {
                Ok(msg) => return Ok((offset, msg.payload[8..].to_vec())),
                Err(err) if attempt < MAX_RETRIES => {
                    eprintln!(
                        "Error loading block (attempt {}/{}): {}. Retrying...",
                        attempt, MAX_RETRIES, err
                    );
                    sleep(RETRY_DELAY).await;
                }
                Err(err) => {
                    return Err(err).context(format!(
                        "Failed to load block after {} attempts",
                        MAX_RETRIES
                    ))
                }
            }
        }

        unreachable!("Loop should always return")
    }

    async fn load_block(&mut self, index: u32, begin: u32, length: u32) -> anyhow::Result<Message> {
        let payload = vec![
            index.to_be_bytes(),
            begin.to_be_bytes(),
            length.to_be_bytes(),
        ]
        .concat();
        let request = Message::new(MessageTag::REQUEST, payload);
        self.send(request).await?;
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::PIECE);
        Ok(msg)
    }
}

pub struct Message {
    length: u32,
    id: MessageTag,
    payload: Vec<u8>,
}

#[derive(Debug, PartialEq, Clone)]
#[repr(u8)]
enum MessageTag {
    BITFIELD = 5,
    INTERESTED = 2,
    UNCHOKE = 1,
    REQUEST = 6,
    PIECE = 7,
}

impl Message {
    fn new(id: MessageTag, payload: Vec<u8>) -> Self {
        let length = (mem::size_of::<MessageTag>() + payload.len()) as u32;
        Self {
            length,
            id,
            payload,
        }
    }

    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(self.length.to_be_bytes());
        bytes.push(self.id.clone() as u8);
        bytes.extend(self.payload.as_slice());
        bytes
    }
}
