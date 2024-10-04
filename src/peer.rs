use serde::{Deserialize, Serialize};
use std::{mem, net::SocketAddrV4};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::torrent::Torrent;

const BLOCK_SIZE: i32 = 16 * 1024;

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

pub struct Peer {
    pub address: SocketAddrV4,
    pub id: Option<[u8; 20]>,
    pub stream: Option<TcpStream>,
}

impl Peer {
    pub async fn handshake(address: SocketAddrV4, info_hash: [u8; 20]) -> anyhow::Result<Self> {
        let mut handshake = Handshake::new(info_hash, *b"00112233445566778899");
        let mut handshake_bytes = bincode::serialize(&handshake)?;

        let mut peer_stream = tokio::net::TcpStream::connect(address).await?;
        peer_stream.write_all(&handshake_bytes).await?;
        peer_stream.read_exact(&mut handshake_bytes).await?;

        handshake = bincode::deserialize(&handshake_bytes)?;
        let peer = Peer {
            address,
            id: Some(handshake.peer_id),
            stream: Some(peer_stream),
        };
        Ok(peer)
    }

    async fn recv(&mut self) -> anyhow::Result<Message> {
        if let Some(stream) = self.stream.as_mut() {
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await?;
            let length = u32::from_be_bytes(buf);

            let mut buf = [0u8; 1];
            stream.read_exact(&mut buf).await?;
            let id: MessageTag = unsafe { mem::transmute(buf[0]) };

            let mut buf = vec![0u8; length as usize - 1];
            stream.read_exact(&mut buf).await?;
            Ok(Message {
                length,
                id,
                payload: buf,
            })
        } else {
            Err(anyhow::anyhow!("No stream"))
        }
    }

    async fn send(&mut self, msg: Message) -> anyhow::Result<()> {
        if let Some(stream) = self.stream.as_mut() {
            stream.write_all(&bincode::serialize(&msg)?).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No stream"))
        }
    }

    pub async fn get_pieces(&mut self) -> anyhow::Result<Vec<usize>> {
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::BITFIELD);
        let bitfield = msg.payload;
        println!("{:?}", bitfield);
        Ok(vec![])
    }

    pub async fn load_piece(&mut self, torrent: &Torrent, index: usize) -> anyhow::Result<Vec<u8>> {
        self.send(Message::new(MessageTag::INTERESTED, vec![]))
            .await?;
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::UNCHOKE);

        let file_size = torrent.info.length;
        let piece_len = torrent.info.piece_length;

        let mut piece = Vec::new();

        Ok(piece)
    }

    async fn load_block(
        &mut self,
        index: usize,
        begin: usize,
        length: usize,
    ) -> anyhow::Result<Message> {
        let payload = vec![
            index.to_be_bytes(),
            begin.to_be_bytes(),
            length.to_be_bytes(),
        ];
        let request = Message::new(MessageTag::REQUEST, payload.concat());
        self.send(request).await?;
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MessageTag::PIECE);
        Ok(msg)
    }
}

#[derive(PartialEq, Serialize)]
#[repr(u8)]
enum MessageTag {
    BITFIELD = 5,
    INTERESTED = 2,
    UNCHOKE = 1,
    REQUEST = 6,
    PIECE = 7,
}

#[derive(Serialize)]
pub struct Message {
    length: u32,
    id: MessageTag,
    payload: Vec<u8>,
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
}
