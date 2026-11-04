use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{SecondsFormat, Utc};

pub(super) const LOG_NAME: &str = "index-service.log";
const OLD_NAME: &str = "index-service.1.log";
const MAX_BYTES: u64 = 1024 * 1024;

/// The service's own diagnostics: volume letters, counts and error kinds only, never names.
#[derive(Clone, Default)]
pub(super) struct Log(Option<Arc<Mutex<LogFile>>>);

impl Log {
    /// Opens `directory/index-service.log`; diagnostics stay off when the folder cannot be used.
    pub fn open(directory: &Path) -> Self {
        Self::open_with_limit(directory, MAX_BYTES)
    }

    pub(super) fn open_with_limit(directory: &Path, max_bytes: u64) -> Self {
        let opened = std::fs::create_dir_all(directory)
            .and_then(|()| LogFile::open(directory.join(LOG_NAME), max_bytes));
        match opened {
            Ok(file) => Self(Some(Arc::new(Mutex::new(file)))),
            Err(error) => {
                tracing::warn!(%error, "opening the index service log");
                Self(None)
            }
        }
    }

    pub fn line(&self, text: impl Display) {
        if let Some(file) = &self.0 {
            file.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .line(&text.to_string());
        }
    }
}

pub(super) struct LogFile {
    path: PathBuf,
    file: File,
    size: u64,
    max_bytes: u64,
    last: String,
    repeats: u64,
}

impl LogFile {
    fn open(path: PathBuf, max_bytes: u64) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            size,
            max_bytes,
            last: String::new(),
            repeats: 0,
        })
    }

    fn line(&mut self, text: &str) {
        if text == self.last {
            self.repeats += 1;
            return;
        }
        if self.repeats != 0 {
            let repeated = format!("previous line repeated {} times", self.repeats);
            self.write(&repeated);
        }
        self.repeats = 0;
        text.clone_into(&mut self.last);
        self.write(text);
    }

    fn write(&mut self, text: &str) {
        let line = format!(
            "{} {text}\n",
            Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
        );
        let length = line.len() as u64;
        // A failed write drops the line: diagnostics must never stop indexing or grow unbounded.
        if self.size.saturating_add(length) > self.max_bytes
            && let Err(error) = self.rotate()
        {
            tracing::warn!(%error, "rotating the index service log");
            return;
        }
        match self.file.write_all(line.as_bytes()) {
            Ok(()) => self.size = self.size.saturating_add(length),
            Err(error) => tracing::warn!(%error, "writing the index service log"),
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        std::fs::rename(&self.path, self.path.with_file_name(OLD_NAME))?;
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(directory: &Path, name: &str) -> String {
        std::fs::read_to_string(directory.join(name)).unwrap_or_default()
    }

    #[test]
    fn lines_carry_a_utc_timestamp_and_append_across_opens() {
        let directory = tempfile::tempdir().unwrap();
        let logs = directory.path().join("logs");
        Log::open(&logs).line("service start pid=1");
        Log::open(&logs).line("service start pid=2");
        let text = read(&logs, LOG_NAME);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let (time, rest) = lines[1].split_once(' ').unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(time).is_ok() && time.ends_with('Z'));
        assert_eq!(rest, "service start pid=2");
    }

    #[test]
    fn the_log_rotates_into_one_old_file_and_stays_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let log = Log::open_with_limit(directory.path(), 400);
        for number in 0..60 {
            log.line(format!("line {number:03}"));
        }
        let current = read(directory.path(), LOG_NAME);
        let old = read(directory.path(), OLD_NAME);
        assert!(current.len() <= 400 && old.len() <= 400);
        assert!(current.ends_with("line 059\n"));
        assert!(!old.is_empty() && !old.contains("line 059"));
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn repeated_lines_collapse_into_one_count() {
        let directory = tempfile::tempdir().unwrap();
        let log = Log::open(directory.path());
        for _ in 0..5 {
            log.line("follow error C: kind=volume_open");
        }
        log.line("heartbeat");
        let text = read(directory.path(), LOG_NAME);
        let rests: Vec<_> = text
            .lines()
            .map(|line| line.split_once(' ').unwrap().1)
            .collect();
        assert_eq!(
            rests,
            [
                "follow error C: kind=volume_open",
                "previous line repeated 4 times",
                "heartbeat"
            ]
        );
    }

    #[test]
    fn an_unusable_folder_turns_the_log_off_without_failing() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("file");
        std::fs::write(&blocker, b"x").unwrap();
        let log = Log::open(&blocker.join("logs"));
        log.line("dropped");
        assert!(log.0.is_none());
    }
}
