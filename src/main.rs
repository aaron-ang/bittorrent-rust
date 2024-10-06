use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};
use std::net::SocketAddrV4;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use bittorrent_starter_rust::decode::decode_bencoded_value;
use bittorrent_starter_rust::peer::Peer;
use bittorrent_starter_rust::torrent::Torrent;

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

#[tokio::main(worker_threads = 5)]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            let decoded = decode_bencoded_value(&value)?;
            println!("{}", decoded);
        }
        Command::Info { torrent } => {
            let torrent = Torrent::new(torrent)?;
            println!("Tracker URL: {}", torrent.announce);
            println!("Length: {}", torrent.info.length);
            println!("Info Hash: {}", hex::encode(torrent.info_hash()?));
            println!("Piece Length: {}", torrent.info.piece_length);
            println!("Piece Hashes:");
            for piece_hash in torrent.pieces() {
                println!("{}", hex::encode(piece_hash));
            }
        }
        Command::Peers { torrent } => {
            let peer_addrs = discover_peers(torrent).await?;
            for addr in peer_addrs {
                println!("{}", addr);
            }
        }
        Command::Handshake { torrent, peer } => {
            let peer = handshake(torrent, peer).await?;
            println!("Peer ID: {}", hex::encode(&peer.id));
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

async fn discover_peers(file_name: PathBuf) -> anyhow::Result<Vec<SocketAddrV4>> {
    let torrent = Torrent::new(file_name)?;
    let peer_addrs = torrent.get_peer_addrs().await?;
    Ok(peer_addrs)
}

async fn handshake(file_name: PathBuf, peer: String) -> anyhow::Result<Peer> {
    let torrent = Torrent::new(file_name)?;
    let address = peer.parse::<SocketAddrV4>()?;
    let peer = Peer::handshake(address, torrent.info_hash()?).await?;
    Ok(peer)
}

async fn download_piece(output: PathBuf, file_name: PathBuf, piece: usize) -> anyhow::Result<()> {
    let torrent = Torrent::new(file_name)?;
    let mut peer = torrent.find_peer_with_piece(piece).await?;
    println!("Found peer: {:?}", peer.address);

    let data = peer.load_piece(&torrent, piece as u32).await?;
    let piece_hash = torrent.pieces()[piece];
    anyhow::ensure!(*piece_hash == *Sha1::digest(&data));

    let mut file = File::create(output).await.unwrap();
    file.write_all(&data).await.unwrap();
    Ok(())
}
