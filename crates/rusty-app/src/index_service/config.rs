use std::io;
use std::path::{Path, PathBuf};

use rusty_store::Search;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Config {
    pub sid: String,
    pub search: Search,
}

impl Config {
    pub fn new(sid: String, search: Search) -> io::Result<Self> {
        let parts: Vec<_> = sid.split('-').collect();
        if parts.len() != 8
            || parts[..4] != ["S", "1", "5", "21"]
            || parts[4..].iter().any(|part| {
                part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || part.parse::<u32>().is_err()
            })
        {
            return Err(invalid("invalid installing account SID"));
        }
        let rid = parts[7]
            .parse::<u32>()
            .map_err(|_| invalid("invalid account RID"))?;
        if !(500..=504).contains(&rid) && rid < 1000 {
            return Err(invalid("the SID identifies a group instead of an account"));
        }
        Ok(Self { sid, search })
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        let config: Self = serde_json::from_slice(bytes).map_err(io::Error::other)?;
        Self::new(config.sid, config.search)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drive {
    pub letter: char,
    pub serial: u64,
    pub fixed: bool,
    pub ntfs: bool,
}

pub(super) fn select_volumes(search: &Search, available: &[Drive]) -> io::Result<Vec<Drive>> {
    let requested = search
        .drives
        .iter()
        .map(|root| drive_letter(root))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(available
        .iter()
        .copied()
        .filter(|drive| {
            drive.ntfs
                && (drive.fixed || (search.removable && !requested.is_empty()))
                && (requested.is_empty() || requested.contains(&drive.letter))
        })
        .collect())
}

pub(super) fn index_path(program_data: &Path, drive: char) -> io::Result<PathBuf> {
    if !drive.is_ascii_alphabetic() {
        return Err(invalid("invalid index drive"));
    }
    Ok(program_data
        .join("Navo")
        .join("index")
        .join(format!("{}.index", drive.to_ascii_uppercase())))
}

fn drive_letter(root: &str) -> io::Result<char> {
    let bytes = root.as_bytes();
    if (bytes.len() == 2 || (bytes.len() == 3 && matches!(bytes[2], b'/' | b'\\')))
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
    {
        return Ok(char::from(bytes[0].to_ascii_uppercase()));
    }
    Err(invalid("service drives must be drive roots"))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn require_user_sid(kind: i32) -> io::Result<()> {
    const SID_TYPE_USER: i32 = 1;
    if kind != SID_TYPE_USER {
        return Err(invalid("the SID does not identify a user account"));
    }
    Ok(())
}
