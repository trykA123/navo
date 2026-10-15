use std::rc::Rc;

use rusty_core::Location;

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

type Read = Rc<dyn Fn(&Location) -> Result<FileText, String>>;
type Write = Rc<dyn Fn(&Location, &Save<'_>) -> Result<Option<FileStamp>, SaveError>>;

pub struct FileHooks {
    pub read: Read,
    pub write: Write,
}

impl FileHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            read: Rc::new(|_| Err("Files cannot be opened here.".to_owned())),
            write: Rc::new(|_, _| Err(SaveError::Failed("Files cannot be saved here.".to_owned()))),
        }
    }
}
