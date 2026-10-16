use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusty_core::Location;
use rusty_core::encoding::TextEncoding;
use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::{ByteSource, LineIndex, Needle};

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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Save<'a> {
    pub text: &'a str,
    pub crlf: bool,
    pub expected: Option<FileStamp>,
    pub encoding: TextEncoding,
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
    pub editable: bool,
    pub stamp: Option<FileStamp>,
}

type Read = Arc<dyn Fn(&Location) -> Result<FileText, String> + Send + Sync>;
type Write =
    Arc<dyn Fn(&Location, &Save<'_>) -> Result<Option<FileStamp>, SaveError> + Send + Sync>;
type ReadHead = Arc<dyn Fn(&Location) -> Result<Head, String> + Send + Sync>;
type OpenPaged = Arc<dyn Fn(&Location) -> Result<Paged, String> + Send + Sync>;
type LoadMemory = Arc<dyn Fn(&Location, FileStamp) -> Option<(MarkSet, Bookmarks)> + Send + Sync>;
type SaveMemory = Arc<dyn Fn(&Location, FileStamp, &MarkSet, &Bookmarks) + Send + Sync>;
type LoadHistory = Arc<dyn Fn() -> SearchHistory + Send + Sync>;
type Remember = Arc<dyn Fn(&Needle) + Send + Sync>;
pub type ReadBytes = Arc<dyn Fn(&Location, u64) -> Result<Vec<u8>, String> + Send + Sync>;

#[derive(Clone)]
pub struct FileHooks {
    pub read: Read,
    pub write: Write,
    pub head: ReadHead,
    pub paged: OpenPaged,
    pub memory: LoadMemory,
    pub save_memory: SaveMemory,
    pub history: LoadHistory,
    pub remember: Remember,
    pub bytes: ReadBytes,
}

impl FileHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            read: Arc::new(|_| Err("Files cannot be opened here.".to_owned())),
            write: Arc::new(|_, _| {
                Err(SaveError::Failed("Files cannot be saved here.".to_owned()))
            }),
            head: Arc::new(|_| Err("Files cannot be opened here.".to_owned())),
            paged: Arc::new(|_| Err("Files cannot be opened here.".to_owned())),
            memory: Arc::new(|_, _| None),
            save_memory: Arc::new(|_, _, _, _| {}),
            history: Arc::new(SearchHistory::default),
            remember: Arc::new(|_| {}),
            bytes: Arc::new(|_, _| Err("Files cannot be opened here.".to_owned())),
        }
    }
}
