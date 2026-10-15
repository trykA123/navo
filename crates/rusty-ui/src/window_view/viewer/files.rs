use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusty_core::Location;
use rusty_core::paging::{ByteSource, LineIndex};

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
}

impl FileText {
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            read_only: false,
            crlf: false,
            stamp: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Save<'a> {
    pub text: &'a str,
    pub crlf: bool,
    pub expected: Option<FileStamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Changed,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAs {
    Text,
    Hex,
    PagedText,
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
}

type Read = Rc<dyn Fn(&Location) -> Result<FileText, String>>;
type Write = Rc<dyn Fn(&Location, &Save<'_>) -> Result<Option<FileStamp>, SaveError>>;
type Pick = Rc<dyn Fn(&Location) -> Result<OpenAs, String>>;
type OpenPaged = Rc<dyn Fn(&Location) -> Result<Paged, String>>;

pub struct FileHooks {
    pub read: Read,
    pub write: Write,
    pub pick: Pick,
    pub paged: OpenPaged,
}

impl FileHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            read: Rc::new(|_| Err("Files cannot be opened here.".to_owned())),
            write: Rc::new(|_, _| Err(SaveError::Failed("Files cannot be saved here.".to_owned()))),
            pick: Rc::new(|_| Err("Files cannot be opened here.".to_owned())),
            paged: Rc::new(|_| Err("Files cannot be opened here.".to_owned())),
        }
    }
}
