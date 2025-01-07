use std::{mem, net::SocketAddr, sync::Arc};

use anyhow::{anyhow, bail, Context, Result};
use bitvec::prelude::*;
use rand::Rng;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    task::JoinSet,
    time::{sleep, Duration},
};

mod extension;
mod handshake;
mod message;

use extension::{ExtensionHeader, ExtensionMessage, ExtensionMessageType};
use handshake::Handshake;
use message::{Message, MessageId};

use crate::torrent::Info;

const BLOCK_SIZE: u32 = 16 * 1024; // 16 KiB

#[derive(Clone)]
pub struct Peer {
    pub address: SocketAddr,
    pub id: [u8; 20],
    pub stream: Arc<Mutex<TcpStream>>,
    pub supports_extension: bool,
    pub metadata_extension_id: Option<u8>,
}

impl Peer {
    pub async fn new(address: SocketAddr, info_hash: [u8; 20]) -> Result<Self> {
        let handshake = Handshake::new(info_hash);
        let handshake_bytes = bincode::serialize(&handshake)?;

        let mut peer_stream = TcpStream::connect(address)
            .await
            .context("Failed to connect to peer")?;
        peer_stream
            .write_all(&handshake_bytes)
            .await
            .context("Failed to send handshake")?;

        let mut response_bytes = vec![0u8; mem::size_of::<Handshake>()];
        peer_stream
            .read_exact(&mut response_bytes)
            .await
            .context("Failed to receive handshake")?;
        let response_handshake: Handshake = bincode::deserialize(&response_bytes)?;

        Ok(Peer {
            address,
            id: response_handshake.peer_id,
            stream: Arc::new(Mutex::new(peer_stream)),
            supports_extension: response_handshake.supports_extension(),
            metadata_extension_id: None,
        })
    }

    pub async fn extension_handshake(&mut self) -> Result<()> {
        let ext_header = ExtensionHeader::new();
        let mut payload = serde_bencode::to_bytes(&ext_header)?;
        payload.insert(0, 0);

        let handshake_msg = Message::new(MessageId::Extension, payload);
        self.send(handshake_msg).await?;
        let reply = self.recv().await?;
        let ext_header = serde_bencode::from_bytes::<ExtensionHeader>(&reply.payload[1..])?;
        self.metadata_extension_id = Some(ext_header.m.ut_metadata);
        Ok(())
    }

    pub async fn extension_metadata(&mut self) -> Result<Info> {
        let ext_msg = ExtensionMessage {
            msg_type: ExtensionMessageType::Request,
            piece: 0,
            total_size: None,
        };
        let mut payload = serde_bencode::to_bytes(&ext_msg)?;
        let extension_msg_id = self
            .metadata_extension_id
            .ok_or_else(|| anyhow!("Metadata extension ID not set"))?;
        payload.insert(0, extension_msg_id);

        let msg = Message::new(MessageId::Extension, payload);
        self.send(msg).await?;
        let reply = self.recv().await?;
        let ext_msg = serde_bencode::from_bytes::<ExtensionMessage>(&reply.payload[1..])?;
        let metadata_piece_len = ext_msg
            .total_size
            .ok_or_else(|| anyhow!("Total size not specified"))?
            as usize;
        let metadata = &reply.payload[reply.payload.len() - metadata_piece_len..];
        let torrent_info = serde_bencode::from_bytes::<Info>(metadata)?;
        Ok(torrent_info)
    }

    async fn send(&self, msg: Message) -> Result<()> {
        let mut stream = self.stream.lock().await;
        stream.write_all(&msg.as_bytes()).await?;
        Ok(())
    }

    async fn recv(&self) -> Result<Message> {
        let mut stream = self.stream.lock().await;
        let length = stream.read_u32().await?;
        let id = MessageId::try_from(stream.read_u8().await?)?;
        let mut payload = vec![0u8; (length - 1) as usize];
        stream.read_exact(&mut payload).await?;
        Ok(Message::new(id, payload))
    }

    pub async fn get_pieces(&self) -> Result<Vec<usize>> {
        let msg = self.recv().await?;
        if msg.id != MessageId::Bitfield {
            bail!("Expected BITFIELD message, got {:?}", msg.id);
        }
        let bitfield = BitVec::<u8, Msb0>::from_vec(msg.payload);
        Ok(bitfield.iter_ones().collect())
    }

    pub async fn prepare_download(&self) -> Result<()> {
        let interested = Message::new(MessageId::Interested, vec![]);
        self.send(interested).await?;
        let msg = self.recv().await?;
        if msg.id != MessageId::Unchoke {
            bail!("Expected UNCHOKE message, got {:?}", msg.id);
        }
        Ok(())
    }

    pub async fn load_piece(&self, index: u32, piece_len: u32) -> Result<Vec<u8>> {
        let mut piece = vec![0u8; piece_len as usize];
        let mut join_set: JoinSet<(usize, Vec<u8>)> = JoinSet::new();

        for offset in (0..piece_len).step_by(BLOCK_SIZE as usize) {
            let length = BLOCK_SIZE.min(piece_len - offset);
            let peer = self.clone();
            join_set.spawn(async move {
                loop {
                    match peer.load_block(index, offset, length).await {
                        Ok(block) => {
                            return (offset as usize, block.payload[8..].to_vec());
                        }
                        Err(err) => {
                            eprintln!(
                                "Error loading block at offset {}: {}. Retrying...",
                                offset, err
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            let (offset, data) = join_result?;
            piece[offset..offset + data.len()].copy_from_slice(&data);
        }

        Ok(piece)
    }

    async fn load_block(&self, index: u32, begin: u32, length: u32) -> Result<Message> {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&begin.to_be_bytes());
        payload.extend_from_slice(&length.to_be_bytes());
        let request = Message::new(MessageId::Request, payload);
        self.send(request).await?;

        let msg = self.recv().await?;
        if msg.id == MessageId::Piece {
            Ok(msg)
        } else {
            bail!("Expected PIECE message, got {:?}", msg.id);
        }
    }

    pub fn gen_peer_id() -> String {
        let peer_id_len = 20;
        (0..peer_id_len)
            .map(|_| rand::thread_rng().gen_range(0..10).to_string())
            .collect()
    }
}
