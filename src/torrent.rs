use std::{collections::HashMap, path::PathBuf};

pub struct Torrent {
    pub announce: reqwest::Url,
    pub info: Info,
}

pub struct Info {
    pub length: i64,
    name: String,
    piece_length: i64,
    pieces: Vec<u8>,
}

fn extract_string(
    key: &str,
    d: &HashMap<Vec<u8>, serde_bencode::value::Value>,
) -> anyhow::Result<String> {
    d.get(key.as_bytes())
        .and_then(|v| match v {
            serde_bencode::value::Value::Bytes(b) => String::from_utf8(b.clone()).ok(),
            _ => None,
        })
        .ok_or(anyhow::anyhow!("Missing field: {}", key))
}

fn extract_bytes(
    key: &str,
    d: &HashMap<Vec<u8>, serde_bencode::value::Value>,
) -> anyhow::Result<Vec<u8>> {
    d.get(key.as_bytes())
        .and_then(|v| match v {
            serde_bencode::value::Value::Bytes(b) => Some(b.clone()),
            _ => None,
        })
        .ok_or(anyhow::anyhow!("Missing field: {}", key))
}

fn extract_dict(
    key: &str,
    d: &HashMap<Vec<u8>, serde_bencode::value::Value>,
) -> anyhow::Result<HashMap<Vec<u8>, serde_bencode::value::Value>> {
    d.get(key.as_bytes())
        .and_then(|v| match v {
            serde_bencode::value::Value::Dict(d) => Some(d.clone()),
            _ => None,
        })
        .ok_or(anyhow::anyhow!("Missing field: {}", key))
}

fn extract_int(
    key: &str,
    d: &HashMap<Vec<u8>, serde_bencode::value::Value>,
) -> anyhow::Result<i64> {
    d.get(key.as_bytes())
        .and_then(|v| match v {
            serde_bencode::value::Value::Int(i) => Some(*i),
            _ => None,
        })
        .ok_or(anyhow::anyhow!("Missing field: {}", key))
}

pub fn parse_torrent_file(file_name: PathBuf) -> anyhow::Result<Torrent> {
    let content = std::fs::read(file_name)?;
    let value = serde_bencode::from_bytes(&content)?;
    match value {
        serde_bencode::value::Value::Dict(d) => {
            let announce = reqwest::Url::parse(&extract_string("announce", &d)?)?;
            let info = extract_dict("info", &d)?;
            let info = Info {
                length: extract_int("length", &info)?,
                name: extract_string("name", &info)?,
                piece_length: extract_int("piece length", &info)?,
                pieces: extract_bytes("pieces", &info)?,
            };
            Ok(Torrent { announce, info })
        }
        _ => Err(anyhow::anyhow!("not a bencoded dictionary")),
    }
}
