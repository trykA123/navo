use rusty_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};

use super::{CacheDb, CacheError};

pub const CONTENT_HASH_BUDGET: u64 = 16 * 1024 * 1024;

pub(super) const TABLE: &str = "content-hash";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentKey {
    pub provider: ProviderId,
    pub path: ProviderPath,
    pub size: u64,
    pub modified: Option<i64>,
    pub file_id: Option<FileIdentity>,
}

impl CacheDb {
    pub fn get_content_hash(&self, key: &ContentKey) -> Result<Option<[u8; 32]>, CacheError> {
        let Some(value) = self.get(TABLE, &encode_key(key))? else {
            return Ok(None);
        };
        Ok(value.try_into().ok())
    }

    pub fn put_content_hash(&mut self, key: &ContentKey, hash: [u8; 32]) -> Result<(), CacheError> {
        self.put(TABLE, &encode_key(key), &hash)
    }
}

pub(super) fn configure(cache: &mut CacheDb) {
    cache.set_budget(TABLE, CONTENT_HASH_BUDGET);
}

fn encode_key(key: &ContentKey) -> Vec<u8> {
    let provider = key.provider.as_str().as_bytes();
    let path = key.path.to_text();
    let path = path.as_bytes();
    let mut out = Vec::with_capacity(60 + provider.len() + path.len());
    put_text(&mut out, provider);
    put_text(&mut out, path);
    out.extend_from_slice(&key.size.to_le_bytes());
    match key.modified {
        Some(modified) => {
            out.push(1);
            out.extend_from_slice(&modified.to_le_bytes());
        }
        None => out.push(0),
    }
    match key.file_id {
        Some(identity) => {
            out.push(1);
            put_identity(&mut out, &identity);
        }
        None => out.push(0),
    }
    out
}

fn put_text(out: &mut Vec<u8>, text: &[u8]) {
    out.extend_from_slice(&(text.len() as u64).to_le_bytes());
    out.extend_from_slice(text);
}

fn put_identity(out: &mut Vec<u8>, identity: &FileIdentity) {
    out.extend_from_slice(&identity.volume.to_le_bytes());
    out.extend_from_slice(&identity.file);
    match identity.kind {
        FileIdentityKind::Native => out.push(0),
        FileIdentityKind::WindowsIndex => out.push(1),
        FileIdentityKind::Fingerprint { folder } => {
            out.push(2);
            out.push(u8::from(folder));
        }
    }
}
