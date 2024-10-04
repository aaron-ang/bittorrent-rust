use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod decode;
mod torrent;

use decode::decode_bencoded_value;
use torrent::parse_torrent_file;

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
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            let decoded_value = decode_bencoded_value(&value)?;
            println!("{}", decoded_value);
        }
        Command::Info {
            torrent: torrent_path,
        } => {
            let torrent = parse_torrent_file(torrent_path)?;
            println!("Tracker URL: {}", torrent.announce);
            println!("Length: {}", torrent.info.length);
        }
    }

    Ok(())
}
