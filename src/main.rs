use anyhow::Ok;
use clap::{Parser, Subcommand};
use std::net::SocketAddrV4;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::form_urlencoded;

use bittorrent_starter_rust::decode::decode_bencoded_value;
use bittorrent_starter_rust::peer::{Handshake, Peer};
use bittorrent_starter_rust::torrent::Torrent;
use bittorrent_starter_rust::tracker::*;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "snake_case")]
enum Command {
    Decode {
        value: String,
    },
    Info {
        torrent: PathBuf,
    },
    Peers {
        torrent: PathBuf,
    },
    Handshake {
        torrent: PathBuf,
        peer: String,
    },
    DownloadPiece {
        #[arg(short)]
        output: PathBuf,
        torrent: PathBuf,
        piece: usize,
    },
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
#[tokio::main(worker_threads = 5)]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            let decoded = decode_bencoded_value(&value)?;
            println!("{}", decoded);
        }
        Command::Info { torrent } => {
            let torrent = parse_torrent_file(torrent)?;
            println!("Tracker URL: {}", torrent.announce);
            println!("Length: {}", torrent.info.length);
            println!("Info Hash: {}", hex::encode(torrent.info_hash()));
            println!("Piece Length: {}", torrent.info.piece_length);
            println!("Piece Hashes:");
            for piece in torrent.pieces() {
                println!("{}", hex::encode(piece));
            }
        }
        Command::Peers { torrent } => {
            let peers = discover_peers(torrent).await?;
            for peer in peers {
                println!("{}", peer);
            }
        }
        Command::Handshake { torrent, peer } => {
            let peer = handshake(torrent, peer).await?;
            println!("Peer ID: {}", hex::encode(&peer.id.unwrap()));
        }
        Command::DownloadPiece {
            output,
            torrent,
            piece,
        } => {
            download_piece(output, torrent, piece).await?;
        }
    }

    Ok(())
}

fn parse_torrent_file(file_name: PathBuf) -> anyhow::Result<Torrent> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    Ok(torrent)
}

async fn discover_peers(file_name: PathBuf) -> anyhow::Result<Vec<SocketAddrV4>> {
    let torrent = parse_torrent_file(file_name)?;
    let peers = discover_peers_impl(&torrent).await?;
    Ok(peers)
}

async fn discover_peers_impl(torrent: &Torrent) -> anyhow::Result<Vec<SocketAddrV4>> {
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
    let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&response.bytes().await?)?;
    Ok(tracker_response.peers())
}

async fn handshake(file_name: PathBuf, peer: String) -> anyhow::Result<Peer> {
    let torrent = parse_torrent_file(file_name)?;
    let peer_address = peer.parse::<SocketAddrV4>()?;
    let peer = handshake_impl(peer_address, torrent.info_hash()).await?;
    Ok(peer)
}

async fn handshake_impl(peer_address: SocketAddrV4, info_hash: [u8; 20]) -> anyhow::Result<Peer> {
    let mut handshake = Handshake::new(info_hash, *b"00112233445566778899");
    let mut handshake_bytes = bincode::serialize(&handshake)?;

    let mut peer = Peer {
        address: peer_address,
        id: None,
        stream: None,
    };
    let mut peer_stream = tokio::net::TcpStream::connect(peer.address).await?;
    peer_stream.write_all(&handshake_bytes).await?;
    peer_stream.read_exact(&mut handshake_bytes).await?;

    handshake = bincode::deserialize(&handshake_bytes)?;
    peer.id = Some(handshake.peer_id);
    peer.stream = Some(peer_stream);
    Ok(peer)
}

async fn download_piece(output: PathBuf, file_name: PathBuf, piece: usize) -> anyhow::Result<()> {
    let torrent = parse_torrent_file(file_name)?;
    let peers = discover_peers_impl(&torrent).await?;
    for peer_address in peers {
        let mut peer = handshake_impl(peer_address, torrent.info_hash()).await?;
        let _ = peer.get_pieces().await?;
        let piece = peer.load_piece(piece).await?;
        let mut file = File::create(output).await.unwrap();
        file.write_all(&piece).await.unwrap();
        break;
    }
    Ok(())
}
