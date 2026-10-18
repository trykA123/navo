use std::path::{Path, PathBuf};

const HEADER_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Copy,
    Move,
}

impl Effect {
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        match self {
            Self::Copy => 5u32.to_le_bytes(),
            Self::Move => 2u32.to_le_bytes(),
        }
    }

    #[must_use]
    pub fn from_bytes(bytes: Option<&[u8]>) -> Self {
        let value = bytes
            .and_then(|bytes| bytes.get(..4))
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
            .unwrap_or(0);
        if value & 2 != 0 {
            Self::Move
        } else {
            Self::Copy
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClipboard {
    pub paths: Vec<PathBuf>,
    pub effect: Effect,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    #[error("The clipboard is in use by another program")]
    Busy,
    #[error("The file clipboard works on Windows only")]
    Unsupported,
    #[error("The clipboard contains malformed file data")]
    Malformed,
    #[error("{0}")]
    Os(String),
}

#[must_use]
pub fn encode_drop_files(paths: &[PathBuf]) -> Vec<u8> {
    let mut bytes = vec![0; HEADER_SIZE];
    bytes[..4].copy_from_slice(&20u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    for path in paths {
        bytes.extend(path_units(path).into_iter().flat_map(u16::to_le_bytes));
        bytes.extend_from_slice(&[0; 2]);
    }
    bytes.extend_from_slice(&[0; 2]);
    if paths.is_empty() {
        bytes.extend_from_slice(&[0; 2]);
    }
    bytes
}

pub(super) fn decode_drop_files(
    bytes: &[u8],
    decode_ansi: impl Fn(&[u8]) -> Result<PathBuf, ClipboardError>,
) -> Result<Vec<PathBuf>, ClipboardError> {
    let offset = usize::try_from(read_dword(bytes, 0)?).map_err(|_| ClipboardError::Malformed)?;
    let wide = read_dword(bytes, 16)? != 0;
    if offset < HEADER_SIZE {
        return Err(ClipboardError::Malformed);
    }
    let data = bytes.get(offset..).ok_or(ClipboardError::Malformed)?;
    if !wide {
        return split_paths(data)?.into_iter().map(decode_ansi).collect();
    }
    if !offset.is_multiple_of(2) {
        return Err(ClipboardError::Malformed);
    }
    let units: Vec<_> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    split_paths(&units)?
        .into_iter()
        .map(path_from_units)
        .collect()
}

fn read_dword(bytes: &[u8], offset: usize) -> Result<u32, ClipboardError> {
    let value = bytes
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .ok_or(ClipboardError::Malformed)?;
    Ok(u32::from_le_bytes(value))
}

fn split_paths<T: Default + PartialEq>(mut data: &[T]) -> Result<Vec<&[T]>, ClipboardError> {
    let zero = T::default();
    let mut paths = Vec::new();
    loop {
        let end = data
            .iter()
            .position(|unit| *unit == zero)
            .ok_or(ClipboardError::Malformed)?;
        if end == 0 {
            if paths.is_empty() && data.get(1) != Some(&zero) {
                return Err(ClipboardError::Malformed);
            }
            return Ok(paths);
        }
        paths.push(&data[..end]);
        data = &data[end + 1..];
    }
}

#[cfg(windows)]
fn path_units(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().collect()
}

#[cfg(unix)]
fn path_units(path: &Path) -> Vec<u16> {
    path.to_string_lossy().encode_utf16().collect()
}

#[cfg(windows)]
fn path_from_units(units: &[u16]) -> Result<PathBuf, ClipboardError> {
    use std::os::windows::ffi::OsStringExt;
    Ok(std::ffi::OsString::from_wide(units).into())
}

#[cfg(unix)]
fn path_from_units(units: &[u16]) -> Result<PathBuf, ClipboardError> {
    String::from_utf16(units)
        .map(PathBuf::from)
        .map_err(|_| ClipboardError::Malformed)
}
