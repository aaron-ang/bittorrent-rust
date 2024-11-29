use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Info {
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
    name: String,
    #[serde(flatten)]
    additional: Additional,
}

impl Info {
    pub fn pieces(&self) -> Vec<Vec<u8>> {
        self.pieces.chunks(20).map(|chunk| chunk.to_vec()).collect()
    }

    pub fn file_len(&self) -> u32 {
        match &self.additional {
            Additional::SingleFile { length } => *length,
            Additional::MultiFile { files } => files.iter().map(|file| file.length).sum(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Additional {
    SingleFile { length: u32 },
    MultiFile { files: Vec<File> },
}

#[derive(Clone, Serialize, Deserialize)]
struct File {
    length: u32,
    path: Vec<String>,
}
