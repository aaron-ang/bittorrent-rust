use anyhow::Context;
use bitvec::prelude::*;
use serde::{Deserialize, Serialize};
use std::{mem, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    task::JoinSet,
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
    pub address: SocketAddr,
    pub id: [u8; 20],
    pub stream: Arc<Mutex<TcpStream>>,
}

impl Peer {
    pub async fn handshake(address: SocketAddr, info_hash: [u8; 20]) -> anyhow::Result<Self> {
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

    pub async fn prepare_download(&mut self) -> anyhow::Result<()> {
        let interested = Message::new(MessageTag::INTERESTED, vec![]);
        self.send(interested).await?;
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::UNCHOKE);
        Ok(())
    }

    pub async fn load_piece(&mut self, torrent: Torrent, index: u32) -> anyhow::Result<Vec<u8>> {
        let piece_len = std::cmp::min(
            torrent.info.piece_length,                         // piece_len
            torrent.len() - index * torrent.info.piece_length, // last piece
        );
        let mut piece = vec![0u8; piece_len as usize];
        let mut join_set = JoinSet::new();

        let spawn = |join_set: &mut JoinSet<_>, mut peer: Peer, offset: u32| {
            let length = BLOCK_SIZE.min(piece_len - offset);
            join_set.spawn(async move {
                match peer.load_block(index, offset, length).await {
                    Ok(msg) => (offset, msg.payload[8..].to_vec()),
                    Err(err) => {
                        eprintln!("Error loading block: {}. Will retry...", err);
                        (offset, vec![])
                    }
                }
            });
        };

        for offset in (0..piece_len).step_by(BLOCK_SIZE as usize) {
            spawn(&mut join_set, self.clone(), offset);
        }

        while let Some(join_result) = join_set.join_next().await {
            let (offset, data) = join_result.context("Task panicked")?;
            if data.is_empty() {
                spawn(&mut join_set, self.clone(), offset);
            } else {
                let start = offset as usize;
                let end = start + data.len();
                piece[start..end].copy_from_slice(&data);
            }
        }

        Ok(piece)
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

#[derive(PartialEq, Clone)]
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
