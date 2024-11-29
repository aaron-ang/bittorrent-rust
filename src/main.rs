use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

use bittorrent_starter_rust::*;

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
        peer_address: SocketAddr,
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
    MagnetParse {
        magnet_link: Url,
    },
    MagnetHandshake {
        magnet_link: Url,
    },
    MagnetInfo {
        magnet_link: Url,
    },
    MagnetDownloadPiece {
        #[arg(short)]
        output: PathBuf,
        magnet_link: Url,
        piece: usize,
    },
    MagnetDownload {
        #[arg(short)]
        output: PathBuf,
        magnet_link: Url,
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
            let mut torrent = Torrent::new(torrent)?;
            let info = torrent.get_info()?.clone();
            println!("Tracker URL: {}", torrent.announce);
            println!("Length: {}", info.file_len());
            println!("Info Hash: {}", hex::encode(torrent.get_info_hash()?));
            println!("Piece Length: {}", info.piece_length);
            println!("Piece Hashes:");
            for piece_hash in info.pieces() {
                println!("{}", hex::encode(piece_hash));
            }
        }
        Command::Peers { torrent } => {
            let mut torrent = Torrent::new(torrent)?;
            for addr in torrent.fetch_peer_addresses().await? {
                println!("{}", addr);
            }
        }
        Command::Handshake {
            torrent,
            peer_address,
        } => {
            let mut torrent = Torrent::new(torrent)?;
            let peer = Peer::new(peer_address, torrent.get_info_hash()?).await?;
            println!("Peer ID: {}", hex::encode(&peer.id));
        }
        Command::DownloadPiece {
            output,
            torrent,
            piece,
        } => {
            let mut torrent = Torrent::new(torrent)?;
            let piece_bytes = torrent.download_piece(piece).await?;
            let mut file = File::create(output).await?;
            file.write_all(&piece_bytes).await?;
        }
        Command::Download { output, torrent } => {
            let mut torrent = Torrent::new(torrent)?;
            let file_bytes = torrent.download().await?;
            let mut file = File::create(output).await?;
            file.write_all(&file_bytes).await?;
        }
        Command::MagnetParse { magnet_link } => {
            let magnet = Magnet::new(magnet_link)?;
            println!("Tracker URL: {}", magnet.tracker_url.unwrap());
            println!("Info Hash: {}", hex::encode(magnet.info_hash));
        }
        Command::MagnetHandshake { magnet_link } => {
            let magnet = Magnet::new(magnet_link)?;
            let peer = magnet.handshake().await?;
            println!("Peer ID: {}", hex::encode(peer.id));
            println!(
                "Peer Metadata Extension ID: {}",
                peer.metadata_extension_id.unwrap()
            );
        }
        Command::MagnetInfo { magnet_link } => {
            let magnet = Magnet::new(magnet_link)?;
            let mut peer = magnet.handshake().await?;
            let info = peer.extension_metadata().await?;
            println!("Tracker URL: {}", magnet.tracker_url.unwrap());
            println!("Length: {}", info.file_len());
            println!("Info Hash: {}", hex::encode(magnet.info_hash));
            println!("Piece Length: {}", info.piece_length);
            println!("Piece Hashes:");
            for piece_hash in info.pieces() {
                println!("{}", hex::encode(piece_hash));
            }
        }
        Command::MagnetDownloadPiece {
            output,
            magnet_link,
            piece,
        } => {
            let magnet = Magnet::new(magnet_link)?;
            let piece_bytes = magnet.download_piece(piece).await?;
            let mut file = File::create(output).await?;
            file.write_all(&piece_bytes).await?;
        }
        Command::MagnetDownload {
            output,
            magnet_link,
        } => {
            let magnet = Magnet::new(magnet_link)?;
            let file_bytes = magnet.download().await?;
            let mut file = File::create(output).await?;
            file.write_all(&file_bytes).await?;
        }
    }

    Ok(())
}
