use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("index pipe frame exceeds 4 MiB")]
    TooLarge,
    #[error("index pipe frame is truncated")]
    Truncated,
    #[error("index pipe frame contains trailing bytes")]
    TrailingBytes,
    #[error("invalid index pipe frame: {0}")]
    Postcard(#[from] postcard::Error),
}

pub fn encode(value: &impl Serialize) -> Result<Vec<u8>, FrameError> {
    let size = postcard::experimental::serialized_size(value)?;
    if size > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    let mut frame = vec![0; size + 4];
    frame[..4].copy_from_slice(&(size as u32).to_le_bytes());
    postcard::to_slice(value, &mut frame[4..])?;
    Ok(frame)
}

pub fn payload_length(prefix: &[u8]) -> Result<usize, FrameError> {
    let prefix = prefix.try_into().map_err(|_| FrameError::Truncated)?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    Ok(length)
}

pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, FrameError> {
    let prefix = frame.get(..4).ok_or(FrameError::Truncated)?;
    let length = payload_length(prefix)?;
    let payload = frame.get(4..length + 4).ok_or(FrameError::Truncated)?;
    if frame.len() != length + 4 {
        return Err(FrameError::TrailingBytes);
    }
    let (value, trailing) = postcard::take_from_bytes(payload)?;
    if !trailing.is_empty() {
        return Err(FrameError::TrailingBytes);
    }
    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryTerm {
    Name(String),
    Path(String),
    Ext(Vec<String>),
    FolderTotalsV1(Vec<String>),
    MemoryV1,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub terms: Vec<QueryTerm>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub request: Request,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub path: String,
    pub flags: u16,
    pub score: u32,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeStatus {
    pub root: String,
    pub entries: u64,
    pub progress: Option<u8>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub volumes: Vec<VolumeStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Hits {
        generation: u64,
        hits: Vec<Hit>,
    },
    Status {
        generation: u64,
        status: Status,
    },
    FolderTotalsV1 {
        generation: u64,
        reply: FolderTotalsReply,
    },
    MemoryV1 {
        generation: u64,
        report: MemoryReport,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderTotals {
    pub bytes: u64,
    pub files: u32,
    pub folders: u32,
    pub complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderTotalsReply {
    pub totals: Vec<Option<FolderTotals>>,
    pub busy: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryPart {
    pub part: u16,
    pub bytes: u64,
    pub items: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryReport {
    pub parts: Vec<MemoryPart>,
    pub busy: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub totals: Option<FolderTotalsReply>,
    pub memory: Option<MemoryReport>,
    pub hits: Vec<Hit>,
    pub status: Status,
}
