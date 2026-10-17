use std::fmt;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum KeymapError {
    Syntax(String),
    UnknownCommand { command: String },
    BadKeystroke { command: String, key: String },
    ContextNotAllowed { command: String, context: String },
    DuplicateKey { key: String, context: String },
    DuplicateEntry { command: String, context: String },
}

impl fmt::Display for KeymapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = formatter;
        todo!()
    }
}

impl std::error::Error for KeymapError {}
