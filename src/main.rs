use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

use bittorrent_starter_rust::decode::decode_bencoded_value;
use bittorrent_starter_rust::magnet::Magnet;
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
        Command::Handshake {
            torrent,
            peer_address,
        } => {
            let peer = handshake(torrent, peer_address).await?;
            println!("Peer ID: {}", hex::encode(&peer.id));
        }
        Command::DownloadPiece {
            output,
            torrent,
            piece,
        } => {
            let torrent = Torrent::new(torrent)?;
            let piece_bytes = torrent.download_piece(piece).await?;
            let mut file = File::create(output).await?;
            file.write_all(&piece_bytes).await?;
        }
        Command::Download { output, torrent } => {
            let torrent = Torrent::new(torrent)?;
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
            println!("Peer ID: {}", hex::encode(&peer.id));
            println!(
                "Peer Metadata Extension ID: {}",
                peer.metadata_extension_id.unwrap()
            );
        }
        Command::MagnetInfo { magnet_link } => {
            let magnet = Magnet::new(magnet_link)?;
            let mut peer = magnet.handshake().await?;
            let metadata = peer.extension_metadata().await?;
            let torrent = Torrent::from_magnet_and_metadata(magnet, metadata)?;
            println!("Tracker URL: {}", torrent.announce);
            println!("Length: {}", torrent.len());
            println!("Info Hash: {}", hex::encode(torrent.info_hash()?));
            println!("Piece Length: {}", torrent.info.piece_length);
            println!("Piece Hashes:");
            for piece_hash in torrent.pieces() {
                println!("{}", hex::encode(piece_hash));
            }
        }
        Command::MagnetDownloadPiece {
            output,
            magnet_link,
            piece,
        } => magnet_download_piece(output, magnet_link, piece).await?,
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

async fn discover_peers(file_name: PathBuf) -> anyhow::Result<Vec<SocketAddr>> {
    let torrent = Torrent::new(file_name)?;
    let peer_addrs = torrent.get_peer_addrs().await?;
    Ok(peer_addrs)
}

async fn handshake(file_name: PathBuf, peer_address: SocketAddr) -> anyhow::Result<Peer> {
    let torrent = Torrent::new(file_name)?;
    let peer = Peer::new(peer_address, torrent.info_hash()?).await?;
    Ok(peer)
}

async fn magnet_download_piece(
    output: PathBuf,
    magnet_link: Url,
    piece: usize,
) -> anyhow::Result<()> {
    let magnet = Magnet::new(magnet_link)?;
    let peer_addrs = magnet.get_peer_addrs().await?;
    // Establish TCP connection with a peer and perform base handshake
    for peer_address in peer_addrs {
        match Peer::new(peer_address, magnet.info_hash).await {
            Ok(mut peer) => {
                let pieces = peer.get_pieces().await?;
                if pieces.contains(&piece) && peer.supports_extension {
                    peer.extension_handshake().await?;
                    let metadata = peer.extension_metadata().await?;
                    let piece = piece as u32;
                    let piece_len = std::cmp::min(
                        metadata.piece_length,                               // piece_len
                        metadata.file_len() - piece * metadata.piece_length, // last piece
                    );
                    peer.prepare_download().await?;
                    let piece_data = peer.load_piece(piece, piece_len).await?;
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
