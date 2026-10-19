use std::io;
use std::path::{Path, PathBuf};

use super::Error;

pub(super) fn profile_hash(profile_dir: &Path) -> Result<String, Error> {
    let absolute = resolve_folder(profile_dir)?;
    let normalized = absolute.to_string_lossy().to_lowercase();
    let hash = normalized
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    Ok(format!("{hash:016x}"))
}

fn resolve_folder(profile_dir: &Path) -> io::Result<PathBuf> {
    let mut existing = std::path::absolute(profile_dir)?;
    let mut missing = Vec::new();
    let mut resolved = loop {
        match existing.canonicalize() {
            Ok(resolved) => break resolved,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(component) = existing.components().next_back() else {
                    return Err(error);
                };
                missing.push(component.as_os_str().to_owned());
                if !existing.pop() {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    };
    for component in missing.iter().rev() {
        if component == ".." {
            resolved.pop();
        } else {
            resolved.push(component);
        }
    }
    Ok(resolved)
}
