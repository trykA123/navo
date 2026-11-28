//! Document text extraction for `docs:on` searches (FR-D11) with zip-bomb and entry-count limits.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::Path;

use navo_core::find::{Skip, xml_text};
use zip::ZipArchive;

use crate::CancelToken;

const MAX_ENTRIES: usize = 512;
const MAX_PART_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const RATIO_THRESHOLD: u64 = 1024 * 1024;
const MAX_RATIO: u64 = 200;
const CHUNK_SIZE: usize = 64 * 1024;

/// Returns whether `ext` is one of the supported Office, OpenDocument, or EPUB extensions.
pub(super) fn is_doc_ext(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("docx")
        || ext.eq_ignore_ascii_case("xlsx")
        || ext.eq_ignore_ascii_case("pptx")
        || ext.eq_ignore_ascii_case("odt")
        || ext.eq_ignore_ascii_case("ods")
        || ext.eq_ignore_ascii_case("odp")
        || ext.eq_ignore_ascii_case("epub")
}

/// Extracts searchable plain text from a zip-based document while enforcing FR-D11 safety limits.
pub(in crate::find) fn doc_text(
    path: &Path,
    ext: &str,
    cancel: &CancelToken,
) -> Result<String, Skip> {
    if cancel.is_cancelled() {
        return Err(Skip::Cancelled);
    }
    let file = File::open(path).map_err(|_| Skip::Document)?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|_| Skip::Document)?;
    if archive.len() > MAX_ENTRIES {
        return Err(Skip::Document);
    }
    let mut text = String::new();
    let mut reader = PartReader::new(cancel);
    for index in 0..archive.len() {
        if cancel.is_cancelled() {
            return Err(Skip::Cancelled);
        }
        let Some(compressed_size) = inspect_part(&mut archive, index, ext)? else {
            continue;
        };
        let entry = archive.by_index(index).map_err(|_| Skip::Document)?;
        reader.read_part(entry, compressed_size)?;
        append_part_text(&reader.part_bytes, &mut text)?;
    }
    Ok(text)
}

struct PartReader<'a> {
    total_read: u64,
    part_bytes: Vec<u8>,
    chunk: Vec<u8>,
    cancel: &'a CancelToken,
}

impl<'a> PartReader<'a> {
    fn new(cancel: &'a CancelToken) -> Self {
        Self {
            total_read: 0,
            part_bytes: Vec::new(),
            chunk: vec![0u8; CHUNK_SIZE],
            cancel,
        }
    }

    fn read_part(&mut self, entry: impl Read, compressed_size: u64) -> Result<(), Skip> {
        self.part_bytes.clear();
        let mut limited = entry.take(MAX_PART_BYTES + 1);
        let mut actual_read: u64 = 0;
        loop {
            if self.cancel.is_cancelled() {
                return Err(Skip::Cancelled);
            }
            let count = match limited.read(&mut self.chunk) {
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Skip::Document),
            };
            if count == 0 {
                return Ok(());
            }
            actual_read = actual_read.saturating_add(count as u64);
            self.total_read = self.total_read.saturating_add(count as u64);
            check_limits(actual_read, self.total_read, compressed_size)?;
            self.part_bytes.extend_from_slice(&self.chunk[..count]);
        }
    }
}

fn check_limits(actual_read: u64, total_read: u64, compressed_size: u64) -> Result<(), Skip> {
    if actual_read > MAX_PART_BYTES || total_read > MAX_TOTAL_BYTES {
        return Err(Skip::Document);
    }
    if actual_read > RATIO_THRESHOLD
        && (compressed_size == 0 || actual_read > compressed_size.saturating_mul(MAX_RATIO))
    {
        return Err(Skip::Document);
    }
    Ok(())
}

/// Inspects entry metadata without decompressing; returns `Ok(Some(compressed_size))` for wanted parts.
fn inspect_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    ext: &str,
) -> Result<Option<u64>, Skip> {
    let raw = archive.by_index_raw(index).map_err(|_| Skip::Document)?;
    if raw.is_dir() {
        return Ok(None);
    }
    let Ok(name) = raw.name() else {
        return Ok(None);
    };
    if !wants_part(ext, name.as_ref()) {
        return Ok(None);
    }
    if raw.encrypted() {
        return Err(Skip::Document);
    }
    Ok(Some(raw.compressed_size()))
}

fn append_part_text(xml: &[u8], out: &mut String) -> Result<(), Skip> {
    let limit = usize::try_from(MAX_TOTAL_BYTES).unwrap_or(usize::MAX);
    if !out.is_empty() && !out.ends_with('\n') {
        if out.len() >= limit {
            return Err(Skip::Document);
        }
        out.push('\n');
    }
    let remaining = limit.saturating_sub(out.len());
    xml_text(xml, out, remaining).map_err(|_| Skip::Document)
}

fn wants_part(ext: &str, name: &str) -> bool {
    if ext.eq_ignore_ascii_case("docx") {
        matches!(
            name,
            "word/document.xml" | "word/footnotes.xml" | "word/endnotes.xml"
        ) || matches_glob(name, "word/header", ".xml")
            || matches_glob(name, "word/footer", ".xml")
    } else if ext.eq_ignore_ascii_case("xlsx") {
        name == "xl/sharedStrings.xml" || matches_glob(name, "xl/worksheets/sheet", ".xml")
    } else if ext.eq_ignore_ascii_case("pptx") {
        matches_glob(name, "ppt/slides/slide", ".xml")
            || matches_glob(name, "ppt/notesSlides/", ".xml")
    } else if ext.eq_ignore_ascii_case("odt")
        || ext.eq_ignore_ascii_case("ods")
        || ext.eq_ignore_ascii_case("odp")
    {
        name == "content.xml"
    } else if ext.eq_ignore_ascii_case("epub") {
        name.rsplit_once('.').is_some_and(|(_, suffix)| {
            suffix.eq_ignore_ascii_case("xhtml")
                || suffix.eq_ignore_ascii_case("html")
                || suffix.eq_ignore_ascii_case("htm")
        })
    } else {
        false
    }
}

fn matches_glob(name: &str, prefix: &str, suffix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .is_some_and(|mid| !mid.contains('/'))
}
