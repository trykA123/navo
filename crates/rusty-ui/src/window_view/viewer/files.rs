use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusty_core::Location;
use rusty_core::encoding::{Encoding, TextEncoding};
use rusty_core::line_ends::LineEnds;
use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::{ByteSource, LineIndex, Needle};
use rusty_fs::archive::ArchiveListing;

use super::pdf::{OpenPdf, OpenUri, no_pdf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileText {
    pub text: String,
    pub read_only: bool,
    pub crlf: bool,
    pub stamp: Option<FileStamp>,
    pub encoding: TextEncoding,
    pub ends: Option<Arc<LineEnds>>,
}

impl FileText {
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            read_only: false,
            crlf: false,
            stamp: None,
            encoding: TextEncoding::default(),
            ends: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Save<'a> {
    pub text: &'a str,
    pub crlf: bool,
    pub expected: Option<FileStamp>,
    pub encoding: TextEncoding,
    pub ends: Option<&'a LineEnds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub stamp: Option<FileStamp>,
    pub ends: Option<LineEnds>,
}

impl From<Option<FileStamp>> for Saved {
    fn from(stamp: Option<FileStamp>) -> Self {
        Self { stamp, ends: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Changed,
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Head {
    pub bytes: Vec<u8>,
    pub len: u64,
}

pub type SharedSource = Arc<dyn ByteSource + Send + Sync>;
pub type Reopen = Arc<dyn Fn() -> Result<Box<dyn ByteSource + Send>, String> + Send + Sync>;
pub type IndexRun = Arc<
    dyn Fn(&AtomicBool, &mut dyn FnMut(&LineIndex)) -> Result<Option<LineIndex>, String>
        + Send
        + Sync,
>;

pub struct Paged {
    pub source: SharedSource,
    pub reopen: Reopen,
    pub index: IndexRun,
    pub summary: Option<String>,
    pub problem: Option<String>,
    pub editable: bool,
    pub stamp: Option<FileStamp>,
}

type Read = Arc<dyn Fn(&Location) -> Result<FileText, String> + Send + Sync>;
type ReadAs = Arc<dyn Fn(&Location, &'static Encoding) -> Result<FileText, String> + Send + Sync>;
type Write = Arc<dyn Fn(&Location, &Save<'_>) -> Result<Saved, SaveError> + Send + Sync>;
type ReadHead = Arc<dyn Fn(&Location) -> Result<Head, String> + Send + Sync>;
type OpenPaged = Arc<dyn Fn(&Location) -> Result<Paged, String> + Send + Sync>;
type OpenPagedAs = Arc<dyn Fn(&Location, &'static Encoding) -> Result<Paged, String> + Send + Sync>;
type ListArchive =
    Arc<dyn Fn(&Location, &AtomicBool) -> Result<ArchiveListing, String> + Send + Sync>;
type ReadArchiveEntry =
    Arc<dyn Fn(&Location, usize, &AtomicBool) -> Result<Vec<u8>, String> + Send + Sync>;
type LoadMemory = Arc<dyn Fn(&Location, FileStamp) -> Option<(MarkSet, Bookmarks)> + Send + Sync>;
type SaveMemory = Arc<dyn Fn(&Location, FileStamp, &MarkSet, &Bookmarks) + Send + Sync>;
type LoadHistory = Arc<dyn Fn() -> SearchHistory + Send + Sync>;
type Remember = Arc<dyn Fn(&Needle) + Send + Sync>;
pub type ReadBytes = Arc<dyn Fn(&Location, u64) -> Result<Vec<u8>, String> + Send + Sync>;
pub type ReadRange = Arc<dyn Fn(&Location, u64, usize) -> Result<Vec<u8>, String> + Send + Sync>;

#[derive(Clone)]
pub struct FileHooks {
    pub read: Read,
    pub read_as: ReadAs,
    pub write: Write,
    pub head: ReadHead,
    pub paged: OpenPaged,
    pub paged_as: OpenPagedAs,
    pub archive: ListArchive,
    pub archive_entry: ReadArchiveEntry,
    pub memory: LoadMemory,
    pub save_memory: SaveMemory,
    pub history: LoadHistory,
    pub remember: Remember,
    pub bytes: ReadBytes,
    pub range: ReadRange,
    pub pdf: OpenPdf,
    pub open_uri: OpenUri,
}

impl FileHooks {
    pub fn read_with(
        &self,
        location: &Location,
        encoding: Option<&'static Encoding>,
    ) -> Result<FileText, String> {
        match encoding {
            Some(encoding) => (self.read_as)(location, encoding),
            None => (self.read)(location),
        }
    }

    pub fn paged_with(
        &self,
        location: &Location,
        encoding: Option<&'static Encoding>,
    ) -> Result<Paged, String> {
        match encoding {
            Some(encoding) => (self.paged_as)(location, encoding),
            None => (self.paged)(location),
        }
    }

    #[must_use]
    pub fn none() -> Self {
        Self {
            read: Arc::new(|_| Err("Files cannot be opened here.".to_owned())),
            read_as: Arc::new(|_, _| Err("Files cannot be opened here.".to_owned())),
            write: Arc::new(|_, _| {
                Err(SaveError::Failed("Files cannot be saved here.".to_owned()))
            }),
            head: Arc::new(|_| Err("Files cannot be opened here.".to_owned())),
            paged: Arc::new(|_| Err("Files cannot be opened here.".to_owned())),
            paged_as: Arc::new(|_, _| Err("Files cannot be opened here.".to_owned())),
            archive: Arc::new(|_, _| Err("Archives cannot be opened here.".to_owned())),
            archive_entry: Arc::new(|_, _, _| Err("Archives cannot be opened here.".to_owned())),
            memory: Arc::new(|_, _| None),
            save_memory: Arc::new(|_, _, _, _| {}),
            history: Arc::new(SearchHistory::default),
            remember: Arc::new(|_| {}),
            bytes: Arc::new(|_, _| Err("Files cannot be opened here.".to_owned())),
            range: Arc::new(|_, _, _| Err("Files cannot be opened here.".to_owned())),
            pdf: no_pdf(),
            open_uri: Arc::new(|_| {}),
        }
    }
}
