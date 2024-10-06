use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{fs::File, io::AsyncWriteExt};

use bittorrent_starter_rust::decode::decode_bencoded_value;
use bittorrent_starter_rust::peer::Peer;
use bittorrent_starter_rust::torrent::Torrent;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
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
    Download {
        #[arg(short)]
        output: PathBuf,
        torrent: PathBuf,
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
            println!("Length: {}", torrent.len());
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
        Command::Download { output, torrent } => {
            download(output, torrent).await?;
        }
    }

    Ok(())
}

async fn discover_peers(file_name: PathBuf) -> anyhow::Result<Vec<SocketAddr>> {
    let torrent = Torrent::new(file_name)?;
    let peer_addrs = torrent.get_peer_addrs().await?;
    Ok(peer_addrs)
}

async fn handshake(file_name: PathBuf, peer: String) -> anyhow::Result<Peer> {
    let torrent = Torrent::new(file_name)?;
    let address = peer.parse::<SocketAddr>()?;
    let peer = Peer::handshake(address, torrent.info_hash()?).await?;
    Ok(peer)
}

async fn download_piece(output: PathBuf, file_name: PathBuf, piece: usize) -> anyhow::Result<()> {
    let torrent = Torrent::new(file_name)?;
    let peer_addrs = torrent.get_peer_addrs().await?;
    let info_hash = torrent.info_hash()?;
    for peer_address in peer_addrs {
        match Peer::handshake(peer_address, info_hash).await {
            Ok(mut peer) => {
                let pieces = peer.get_pieces().await?;
                if pieces.contains(&piece) {
                    peer.prepare_download().await?;
                    let piece_data = peer.load_piece(torrent, piece as u32).await?;
                    let mut file = File::create(output).await?;
                    file.write_all(&piece_data).await?;
                    return Ok(());
                }
            }
            Err(e) => eprintln!("{} -> {}", peer_address, e),
        }
    }
    Err(anyhow::anyhow!("Could not find peer"))
}

async fn download(output: PathBuf, file_name: PathBuf) -> anyhow::Result<()> {
    let torrent = Torrent::new(file_name)?;
    let file_bytes = torrent.download().await?;
    let mut file = File::create(output).await?;
    file.write_all(&file_bytes).await?;
    Ok(())
}
