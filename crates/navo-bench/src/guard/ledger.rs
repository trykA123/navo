//! Tokens of this tool's runs that are not recycled yet, in one file beside the runs.
//! Stale cleanup trusts a run only when its marker token is listed here. No paths are stored.
//!
//! The ledger sits in the run roots' parent. `placement::create` has already refused a parent
//! that is a drive or system root, or that lies in or holds the owner's folder, before the first
//! `add`, so a ledger write never reaches the owner's data. A file at the ledger path that is
//! not a valid ledger (magic line, then token lines) is never replaced.

use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use super::{GuardError, PathRole, act, platform, write};

pub(super) const LEDGER_FILE: &str = ".navo-bench-ledger";
/// First line of every ledger; a file without it is not ours.
pub(super) const MAGIC: &str = "navo-bench-ledger-v1\n";
const TOKEN_LEN: usize = 32;
const LINE_LEN: usize = TOKEN_LEN * 2 + 1;
const MAX_BYTES: usize = 64 * 1024;
const HEX: &[u8; 16] = b"0123456789abcdef";

pub(super) type Token = [u8; TOKEN_LEN];

/// The listed tokens; a missing, linked, oversized or malformed ledger is an error.
pub(super) fn read(parent: &Path, role: PathRole) -> Result<Vec<Token>, GuardError> {
    load(parent, role)?.ok_or(GuardError::Ledger(role))
}

/// `None` only when no entry exists at the ledger path; every other failure is an error.
fn load(parent: &Path, role: PathRole) -> Result<Option<Vec<Token>>, GuardError> {
    let path = parent.join(LEDGER_FILE);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(GuardError::Ledger(role)),
    };
    if platform::is_reparse(&metadata) || !metadata.is_file() {
        return Err(GuardError::Ledger(role));
    }
    let file = File::open(&path).map_err(|_| GuardError::Ledger(role))?;
    act::confirm_handle(&file, &path, role).map_err(|_| GuardError::Ledger(role))?;
    let limit = u64::try_from(MAX_BYTES + 1).map_err(|_| GuardError::Ledger(role))?;
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| GuardError::Ledger(role))?;
    parse(&bytes).map(Some).ok_or(GuardError::Ledger(role))
}

/// Lists `token`. An absent ledger starts a new one; any other file that is not a valid
/// ledger is an error and stays untouched.
pub(super) fn add(parent: &Path, token: &Token, role: PathRole) -> Result<(), GuardError> {
    let mut tokens = load(parent, role)?.unwrap_or_default();
    if !tokens.contains(token) {
        tokens.push(*token);
    }
    store(parent, &tokens, role)
}

pub(super) fn remove(parent: &Path, token: &Token, role: PathRole) -> Result<(), GuardError> {
    let mut tokens = read(parent, role)?;
    let before = tokens.len();
    tokens.retain(|listed| listed != token);
    if tokens.len() == before {
        return Ok(());
    }
    store(parent, &tokens, role)
}

fn parse(bytes: &[u8]) -> Option<Vec<Token>> {
    let body = bytes.strip_prefix(MAGIC.as_bytes())?;
    if bytes.len() > MAX_BYTES || !body.len().is_multiple_of(LINE_LEN) {
        return None;
    }
    body.chunks(LINE_LEN).map(parse_line).collect()
}

fn parse_line(line: &[u8]) -> Option<Token> {
    let (hex, end) = line.split_at_checked(TOKEN_LEN * 2)?;
    if end != b"\n" {
        return None;
    }
    let mut token = [0_u8; TOKEN_LEN];
    for (byte, pair) in token.iter_mut().zip(hex.chunks(2)) {
        let text = std::str::from_utf8(pair).ok()?;
        if text
            .bytes()
            .any(|c| !matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        {
            return None;
        }
        *byte = u8::from_str_radix(text, 16).ok()?;
    }
    Some(token)
}

/// Writes a fresh temporary file beside the ledger, checks that the file now at the ledger
/// path is absent or a valid ledger, and only then renames the temporary file over it.
fn store(parent: &Path, tokens: &[Token], role: PathRole) -> Result<(), GuardError> {
    let mut text = String::with_capacity(MAGIC.len() + tokens.len() * LINE_LEN);
    text.push_str(MAGIC);
    for token in tokens {
        for byte in token {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0xF)]));
        }
        text.push('\n');
    }
    if text.len() > MAX_BYTES {
        return Err(GuardError::Ledger(role));
    }
    let temporary = temporary_path(parent, role)?;
    let mut file = write::open_new(&temporary).map_err(|_| GuardError::Io(role))?;
    act::confirm_new_file(&file, &temporary, role)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| GuardError::Io(role))
        .and_then(|()| load(parent, role).map(drop))
        .and_then(|()| {
            write::replace_ledger(&temporary, &parent.join(LEDGER_FILE))
                .map_err(|_| GuardError::Io(role))
        });
    if let Err(error) = written {
        let _ = act::discard_new_file(&file, role);
        return Err(error);
    }
    let directory = platform::pin(parent).map_err(|_| GuardError::Io(role))?;
    platform::sync_directory(&directory).map_err(|_| GuardError::Io(role))
}

fn temporary_path(parent: &Path, role: PathRole) -> Result<PathBuf, GuardError> {
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|_| GuardError::Entropy(role))?;
    let name = format!("{LEDGER_FILE}-{:016x}.tmp", u64::from_le_bytes(nonce));
    Ok(parent.join(name))
}

#[cfg(all(test, unix))]
pub(super) fn tokens_in(parent: &Path) -> Result<Vec<Token>, GuardError> {
    read(parent, PathRole::StaleRun)
}
