use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod decode;
mod torrent;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Decode { value: String },
    Info { torrent_path: PathBuf },
    Peers { torrent_path: PathBuf },
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            decode::decode_bencoded_value(&value)?;
        }
        Command::Info { torrent_path } => {
            torrent::parse_torrent_file(torrent_path)?;
        }
        Command::Peers { torrent_path } => {
            torrent::discover_peers(torrent_path)?;
        }
    }

    Ok(())
}
