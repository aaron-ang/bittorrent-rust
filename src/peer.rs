use serde::{Deserialize, Serialize};
use std::{mem, net::SocketAddrV4};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

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
    async fn recv(&mut self) -> anyhow::Result<Message> {
        if let Some(stream) = self.stream.as_mut() {
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await?;
            let length = u32::from_be_bytes(buf);

            let mut buf = [0u8; 1];
            stream.read_exact(&mut buf).await?;
            let id: MESSAGE = unsafe { mem::transmute(buf[0]) };

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
        anyhow::ensure!(msg.id == MESSAGE::BITFIELD);
        Ok(vec![])
    }

    pub async fn load_piece(&mut self, piece: usize) -> anyhow::Result<Vec<u8>> {
        self.send(Message::new(MESSAGE::PIECE, vec![])).await?;
        let msg = self.recv().await?;
        anyhow::ensure!(msg.id == MESSAGE::UNCHOKE);
        todo!("load piece")
    }
}

#[derive(PartialEq, Serialize)]
#[repr(u8)]
enum MESSAGE {
    BITFIELD = 5,
    INTERESTED = 2,
    UNCHOKE = 1,
    REQUEST = 6,
    PIECE = 7,
}

#[derive(Serialize)]
pub struct Message {
    length: u32,
    id: MESSAGE,
    payload: Vec<u8>,
}

impl Message {
    fn new(id: MESSAGE, payload: Vec<u8>) -> Self {
        let length = (mem::size_of::<MESSAGE>() + payload.len()) as u32;
        Self {
            length,
            id,
            payload,
        }
    }
}
