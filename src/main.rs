use clap::{Parser, Subcommand};
use std::net::SocketAddrV4;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::form_urlencoded;

use bittorrent_starter_rust::decode::decode_bencoded_value;
use bittorrent_starter_rust::peer::Handshake;
use bittorrent_starter_rust::torrent::Torrent;
use bittorrent_starter_rust::tracker::*;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Decode { value: String },
    Info { torrent: PathBuf },
    Peers { torrent: PathBuf },
    Handshake { torrent: PathBuf, peer: String },
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            decode_bencoded_value(&value)?;
        }
        Command::Info { torrent } => {
            parse_torrent_file(torrent)?;
        }
        Command::Peers { torrent } => {
            discover_peers(torrent).await?;
        }
        Command::Handshake { torrent, peer } => {
            handshake(torrent, peer).await?;
        }
    }

    Ok(())
}

fn parse_torrent_file(file_name: PathBuf) -> anyhow::Result<()> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    println!("Tracker URL: {}", torrent.announce);
    println!("Length: {}", torrent.info.length);
    println!("Info Hash: {}", hex::encode(torrent.info_hash()));
    println!("Piece Length: {}", torrent.info.piece_length);
    println!("Piece Hashes:");
    for piece in torrent.info.pieces.chunks_exact(20) {
        println!("{}", hex::encode(piece));
    }
    Ok(())
}

async fn discover_peers(file_name: PathBuf) -> anyhow::Result<()> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    let info_hash_str = form_urlencoded::byte_serialize(&torrent.info_hash()).collect::<String>();
    let request = TrackerRequest {
        peer_id: String::from("00112233445566778899"),
        port: 6881,
        uploaded: 0,
        downloaded: 0,
        left: torrent.info.length,
        compact: 1,
    };
    let params = serde_urlencoded::to_string(&request)?;
    let url = format!(
        "{}?{}&info_hash={}",
        torrent.announce, params, info_hash_str
    );
    let response = reqwest::get(url).await?;
    let tracker = serde_bencode::from_bytes::<Tracker>(&response.bytes().await?)?;
    for peer in tracker.peers() {
        println!("{}", peer);
    }
    Ok(())
}

async fn handshake(file_name: PathBuf, peer: String) -> anyhow::Result<()> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    let info_hash = torrent.info_hash();

    let mut handshake = Handshake::new(info_hash, *b"00112233445566778899");
    let mut handshake_bytes = bincode::serialize(&handshake)?;

    let peer = peer.parse::<SocketAddrV4>()?;
    let mut peer_stream = tokio::net::TcpStream::connect(peer).await?;

    peer_stream.write_all(&handshake_bytes).await?;
    peer_stream.read_exact(&mut handshake_bytes).await?;

    handshake = bincode::deserialize(&handshake_bytes)?;
    println!("Peer ID: {}", hex::encode(&handshake.peer_id));
    Ok(())
}
