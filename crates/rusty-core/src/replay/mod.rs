mod text;

use crate::{AppState, Command, CommandError, Effect, Event, Location, apply, apply_event};

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

// `records[start..]` is the log; the dropped prefix is compacted away in bulk.
#[derive(Debug, Clone)]
pub struct CommandLog {
    home: Location,
    records: Vec<Record>,
    start: usize,
    dropped: u64,
}

impl PartialEq for CommandLog {
    fn eq(&self, other: &Self) -> bool {
        self.home == other.home
            && self.dropped == other.dropped
            && self.records() == other.records()
    }
}

impl Eq for CommandLog {}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogError {
    #[error("the log has version {found}; this build reads up to version {supported}")]
    TooNew { found: u32, supported: u32 },
    #[error("the text is not a Navo command log")]
    BadHeader,
    #[error("line {line}: {message}")]
    Line { line: usize, message: String },
}

impl CommandLog {
    pub const VERSION: u32 = 1;
    pub const LIMIT: usize = 10_000;

    #[must_use]
    pub fn new(home: Location) -> Self {
        Self {
            home,
            records: Vec::new(),
            start: 0,
            dropped: 0,
        }
    }

    #[must_use]
    pub fn home(&self) -> &Location {
        &self.home
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub(super) fn set_dropped(&mut self, dropped: u64) {
        self.dropped = dropped;
    }

    #[must_use]
    pub fn records(&self) -> &[Record] {
        self.records.get(self.start..).unwrap_or_default()
    }

    pub fn push(&mut self, after_ms: u64, entry: LogEntry) {
        self.records.push(Record { after_ms, entry });
        if self.records.len() - self.start > Self::LIMIT {
            self.start += 1;
            self.dropped = self.dropped.saturating_add(1);
        }
        if self.start >= Self::LIMIT {
            self.records.drain(..self.start);
            self.start = 0;
        }
    }

    #[must_use]
    pub fn replay(&self) -> AppState {
        let mut state = AppState::new(self.home.clone());
        for record in self.records() {
            match &record.entry {
                LogEntry::Command(command) => {
                    let _ = apply(&mut state, command);
                }
                LogEntry::Event(event) => {
                    apply_event(&mut state, event.clone());
                }
            }
        }
        state
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
        Self {
            state: AppState::new(home.clone()),
            log: CommandLog::new(home),
            last_ms: None,
        }
    }

    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    #[must_use]
    pub fn log(&self) -> &CommandLog {
        &self.log
    }

    pub fn command(&mut self, at_ms: u64, command: Command) -> Result<Vec<Effect>, CommandError> {
        let result = apply(&mut self.state, &command);
        let after = self.tick(at_ms);
        self.log.push(after, LogEntry::Command(command));
        result
    }

    pub fn event(&mut self, at_ms: u64, event: Event) -> Vec<Effect> {
        let effects = apply_event(&mut self.state, event.clone());
        let after = self.tick(at_ms);
        self.log.push(after, LogEntry::Event(event));
        effects
    }

    fn tick(&mut self, at_ms: u64) -> u64 {
        let after = self.last_ms.map_or(0, |last| at_ms.saturating_sub(last));
        self.last_ms = Some(at_ms);
        after
    }
}

#[cfg(test)]
mod tests;
