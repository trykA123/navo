use std::io;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

use crate::{GitError, ObjectId};

pub(super) fn parse_header(header: &[u8]) -> Result<(&[u8], u64), GitError> {
    let mut fields = header.split(|byte| *byte == b' ');
    let _oid = fields.next().ok_or_else(protocol)?;
    let kind = fields.next().ok_or_else(protocol)?;
    let size = fields.next().ok_or_else(protocol)?;
    if fields.next().is_some() {
        return Err(protocol());
    }
    let size = std::str::from_utf8(size)
        .ok()
        .and_then(|size| size.parse::<u64>().ok())
        .ok_or_else(protocol)?;
    Ok((kind, size))
}

pub(super) fn protocol() -> GitError {
    GitError::Protocol {
        detail: "malformed cat-file response".to_owned(),
    }
}

pub(super) fn hex(id: &ObjectId) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let bytes = id.as_bytes();
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

pub(super) fn io_error(error: io::Error) -> GitError {
    GitError::Io {
        path: PathBuf::from("git cat-file"),
        kind: error.kind(),
    }
}

pub(super) fn stop_locked(child: &Arc<Mutex<Child>>) {
    if let Ok(mut child) = child.lock() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub(super) fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
