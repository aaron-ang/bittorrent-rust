use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Torrent {
    announce: String,
    info: Info,
}

#[derive(Serialize, Deserialize)]
struct Info {
    length: i64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: i64,
    #[serde(with = "serde_bytes")]
    pieces: Vec<u8>,
}

pub fn parse_torrent_file(file_name: PathBuf) -> anyhow::Result<()> {
    let content = std::fs::read(file_name)?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&content)?;
    println!("Tracker URL: {}", torrent.announce);
    println!("Length: {}", torrent.info.length);
    println!(
        "Info Hash: {}",
        hex::encode(Sha1::digest(serde_bencode::to_bytes(&torrent.info)?))
    );
    println!("Piece Length: {}", torrent.info.piece_length);
    println!("Piece Hashes:");
    for piece in torrent.info.pieces.chunks(20) {
        println!("{}", hex::encode(piece));
    }
    Ok(())
}
