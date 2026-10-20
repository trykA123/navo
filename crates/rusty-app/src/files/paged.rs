use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusty_core::Location;
use rusty_core::encoding::{Encoding, TextEncoding, choose, detect, is_lossy, looks_binary};
use rusty_core::paging::{ByteSource, Laid, TextLayout, read_full};
use rusty_fs::{PagedFile, index_lines};
use rusty_ui::window_view::{Head, Paged};

use super::code_page::ansi;
use super::summary::{SUMMARY_HEAD, binary_summary};
use super::{MAX_VIEW, SNIFF, path_of, stamp_of};

const DETECT_HEAD: usize = 64 * 1024;
const TAIL_TRIM: usize = 4;

fn sniff(path: &Path) -> Result<Head, String> {
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
    Ok(Head {
        bytes: head,
        len: metadata.len(),
    })
}

pub fn head(location: &Location) -> Result<Head, String> {
    sniff(&path_of(location)?)
}

fn open_source(path: &Path, layout: TextLayout) -> Result<Box<dyn ByteSource + Send>, String> {
    Ok(Box::new(Laid::new(PagedFile::open(path)?, layout)))
}

fn text_encoding(head: &[u8], chosen: Option<&'static Encoding>) -> TextEncoding {
    match chosen {
        Some(encoding) => choose(head, encoding),
        None => detect(head, ansi()),
    }
}

fn problem_in(head: &[u8], is_whole: bool, encoding: &TextEncoding) -> Option<String> {
    let probe = if is_whole {
        head
    } else {
        &head[..head.len().saturating_sub(TAIL_TRIM) & !1]
    };
    is_lossy(probe, encoding).then(|| {
        format!(
            "Some bytes are not valid as {}: they show as \u{FFFD}.",
            encoding.label()
        )
    })
}

pub fn paged(location: &Location) -> Result<Paged, String> {
    open(location, None)
}

pub fn paged_as(location: &Location, encoding: &'static Encoding) -> Result<Paged, String> {
    open(location, Some(encoding))
}

fn open(location: &Location, chosen: Option<&'static Encoding>) -> Result<Paged, String> {
    let path: PathBuf = path_of(location)?;
    let sniffed = sniff(&path)?;
    let stamp = stamp_of(&path);
    let raw = PagedFile::open(&path)?;
    let mut head = vec![0u8; DETECT_HEAD];
    let count = read_full(&raw, 0, &mut head)?;
    head.truncate(count);
    let encoding = text_encoding(&head, chosen);
    let layout = TextLayout::of(encoding);
    let source = Arc::new(Laid::new(raw, layout));
    let reopen_path = path.clone();
    Ok(Paged {
        source,
        reopen: Arc::new(move || open_source(&reopen_path, layout)),
        index: Arc::new(move |cancel, progress| index_lines(&path, layout, cancel, progress)),
        summary: binary_summary(&head[..head.len().min(SUMMARY_HEAD)]),
        problem: problem_in(&head, sniffed.len <= DETECT_HEAD as u64, &encoding),
        editable: !looks_binary(&sniffed.bytes) && sniffed.len <= MAX_VIEW,
        stamp,
    })
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_paths;
