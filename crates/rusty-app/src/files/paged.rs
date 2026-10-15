use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusty_core::Location;
use rusty_core::paging::{ByteSource, read_full};
use rusty_fs::{PagedFile, index_lines};
use rusty_ui::window_view::{OpenAs, Paged};

use super::summary::{SUMMARY_HEAD, binary_summary};
use super::{MAX_VIEW, SNIFF, path_of};

fn looks_binary(head: &[u8]) -> bool {
    head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) || head.contains(&0)
}

fn sniff(path: &Path) -> Result<(u64, bool), String> {
    let mut file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Cannot open it: {error}"))?;
    if !metadata.is_file() {
        return Err("Only files open here.".to_owned());
    }
    let mut head = Vec::with_capacity(SNIFF);
    file.by_ref()
        .take(SNIFF as u64)
        .read_to_end(&mut head)
        .map_err(|error| format!("Cannot read it: {error}"))?;
    Ok((metadata.len(), looks_binary(&head)))
}

pub fn pick(location: &Location) -> Result<OpenAs, String> {
    let (len, binary) = sniff(&path_of(location)?)?;
    Ok(if binary {
        OpenAs::Hex
    } else if len > MAX_VIEW {
        OpenAs::PagedText
    } else {
        OpenAs::Text
    })
}

fn open_source(path: &Path) -> Result<Box<dyn ByteSource + Send>, String> {
    Ok(Box::new(PagedFile::open(path)?))
}

pub fn paged(location: &Location) -> Result<Paged, String> {
    let path: PathBuf = path_of(location)?;
    let (len, binary) = sniff(&path)?;
    let source = Arc::new(PagedFile::open(&path)?);
    let mut head = vec![0u8; SUMMARY_HEAD];
    let count = read_full(source.as_ref(), 0, &mut head)?;
    head.truncate(count);
    let reopen_path = path.clone();
    Ok(Paged {
        source,
        reopen: Arc::new(move || open_source(&reopen_path)),
        index: Arc::new(move |cancel, progress| index_lines(&path, cancel, progress)),
        summary: binary_summary(&head),
        editable: !binary && len <= MAX_VIEW,
    })
}

#[cfg(test)]
mod tests;
