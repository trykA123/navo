mod text;

use crate::{AppState, Command, CommandError, Effect, Event, Location};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEntry {
    Command(Command),
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub after_ms: u64,
    pub entry: LogEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLog {
    home: Location,
    records: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogError {
    #[error("the log has version {found}; this build reads up to version {supported}")]
    TooNew { found: u32, supported: u32 },
    #[error("the text is not a Ferrite command log")]
    BadHeader,
    #[error("line {line}: {message}")]
    Line { line: usize, message: String },
}

impl CommandLog {
    pub const VERSION: u32 = 1;
    pub const LIMIT: usize = 10_000;

    #[must_use]
    pub fn new(home: Location) -> Self {
        todo!()
    }

    #[must_use]
    pub fn home(&self) -> &Location {
        todo!()
    }

    #[must_use]
    pub fn records(&self) -> &[Record] {
        todo!()
    }

    pub fn push(&mut self, after_ms: u64, entry: LogEntry) {
        todo!()
    }

    #[must_use]
    pub fn replay(&self) -> AppState {
        todo!()
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        text::write(self)
    }

    pub fn parse(text: &str) -> Result<Self, LogError> {
        text::read(text)
    }
}

pub struct Recorder {
    state: AppState,
    log: CommandLog,
    last_ms: Option<u64>,
}

impl Recorder {
    #[must_use]
    pub fn new(home: Location) -> Self {
        todo!()
    }

    #[must_use]
    pub fn state(&self) -> &AppState {
        todo!()
    }

    #[must_use]
    pub fn log(&self) -> &CommandLog {
        todo!()
    }

    pub fn command(&mut self, at_ms: u64, command: Command) -> Result<Vec<Effect>, CommandError> {
        todo!()
    }

    pub fn event(&mut self, at_ms: u64, event: Event) -> Vec<Effect> {
        todo!()
    }
}

#[cfg(test)]
mod tests;
