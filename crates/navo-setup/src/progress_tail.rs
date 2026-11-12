//! Transient log tailing: no handle survives a poll, and rotation is followed (IP3, IP5).

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::progress_log::{Estimator, InstallStage, PackageWeights};

/// How long Finalizing may keep a percentage without a new log line (IP5, owner: keep the grace).
const FINALIZE_STALL_GRACE: Duration = Duration::from_secs(2);

/// The most one poll reads; Velopack rotates its own log at 1 MB.
const MAX_POLL_BYTES: u64 = 2 * 1024 * 1024;

/// Reads the child's log between polls and turns it into the bar's percentage and phase.
pub struct LogTailer {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    estimator: Estimator,
}

impl LogTailer {
    /// Tails `path` for one child; a fresh install also copies the package first.
    pub fn new(path: &Path, upgrade: bool, weights: Option<PackageWeights>) -> Self {
        Self {
            path: path.to_path_buf(),
            offset: 0,
            partial: Vec::new(),
            estimator: Estimator::new(upgrade, weights),
        }
    }

    /// Reads the new complete lines and returns what the bar should draw.
    pub fn snapshot(&mut self, now: Duration) -> (Option<u8>, Option<InstallStage>) {
        self.poll(now);
        let stage = self.estimator.stage();
        let idle = now.saturating_sub(self.estimator.advanced_at());
        let stalled = stage == Some(InstallStage::Finalizing) && idle >= FINALIZE_STALL_GRACE;
        let percent = if stalled {
            None
        } else {
            self.estimator.percent()
        };
        (percent, stage)
    }

    /// Feeds every line the log has completed since the last poll.
    fn poll(&mut self, now: Duration) {
        let Some(bytes) = self.read_new() else {
            return;
        };
        self.partial.extend_from_slice(&bytes);
        let Some(last) = self.partial.iter().rposition(|byte| *byte == b'\n') else {
            return;
        };
        let text = String::from_utf8_lossy(&self.partial[..=last]).into_owned();
        self.partial.drain(..=last);
        for line in text.lines() {
            self.estimator.feed_line(line, now);
        }
    }

    /// Reads the unread bytes, draining the rotated log first when the main one shrank.
    fn read_new(&mut self) -> Option<Vec<u8>> {
        let mut file = open_shared(&self.path).ok()?;
        let mut bytes = Vec::new();
        if file.metadata().ok()?.len() < self.offset {
            bytes.extend(self.rotated_tail());
            self.offset = 0;
        }
        file.seek(SeekFrom::Start(self.offset)).ok()?;
        let read = file
            .by_ref()
            .take(MAX_POLL_BYTES)
            .read_to_end(&mut bytes)
            .ok()?;
        self.offset = self.offset.saturating_add(u64::try_from(read).ok()?);
        Some(bytes)
    }

    /// The unread tail of `<log>.old`, which Velopack's 1 MB rotation leaves behind.
    fn rotated_tail(&self) -> Vec<u8> {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".old");
        let Ok(mut file) = open_shared(Path::new(&name)) else {
            return Vec::new();
        };
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(self.offset)).is_ok() {
            // A short read still carries the lines that did arrive before the error.
            let _ = file.by_ref().take(MAX_POLL_BYTES).read_to_end(&mut bytes);
        }
        bytes
    }
}

/// Opens the log for one transient read, sharing it with the child that writes it.
fn open_shared(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    options.open(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const HEADER: &str = "[Setup:7] [10:00:00] [INFO] Extracting 2 app files";
    const HEADER_TAIL: &str = " to \"C:\\Navo\\current\"...\n";
    const FIRST: &str =
        "[Setup:7] [10:00:01] [DEBUG]     0 Extracting 'lib/net8/navo.exe' to \"C:\\n.exe\"\n";
    const BACKING_UP: &str = "[Setup:7] [10:00:02] [INFO] Backing up current dir\n";
    const SHORTCUTS: &str = "Creating shortcuts...\n";

    /// A tailer over a log in a fresh temporary folder.
    fn tailer(dir: &Path) -> (LogTailer, PathBuf) {
        let path = dir.join("velopack.log");
        (LogTailer::new(&path, true, None), path)
    }

    #[test]
    fn a_partial_line_is_held_until_its_newline_arrives() {
        let dir = tempfile::tempdir().expect("temp");
        let (mut tailer, path) = tailer(dir.path());
        fs::write(&path, HEADER).expect("write the half line");
        assert_eq!(
            tailer.snapshot(Duration::from_secs(1)),
            (None, None),
            "an unterminated line is not evidence of anything"
        );
        fs::write(&path, format!("{HEADER}{HEADER_TAIL}")).expect("complete the line");
        let (percent, stage) = tailer.snapshot(Duration::from_secs(2));
        assert_eq!(stage, Some(InstallStage::Extracting));
        assert_eq!(percent, Some(1));
    }

    #[test]
    fn a_missing_log_leaves_the_bar_indeterminate() {
        let dir = tempfile::tempdir().expect("temp");
        let (mut tailer, _) = tailer(dir.path());
        assert_eq!(tailer.snapshot(Duration::from_secs(1)), (None, None));
    }

    #[test]
    fn a_directory_in_place_of_the_log_is_not_fatal() {
        let dir = tempfile::tempdir().expect("temp");
        let mut tailer = LogTailer::new(dir.path(), true, None);
        assert_eq!(tailer.snapshot(Duration::from_secs(1)), (None, None));
    }

    #[test]
    fn rotation_drains_the_old_tail_before_reading_the_new_log() {
        let dir = tempfile::tempdir().expect("temp");
        let (mut tailer, path) = tailer(dir.path());
        fs::write(&path, format!("{HEADER}{HEADER_TAIL}")).expect("write");
        tailer.snapshot(Duration::from_secs(1));
        fs::write(&path, format!("{HEADER}{HEADER_TAIL}{FIRST}{BACKING_UP}")).expect("grow");
        let old = path.with_extension("log.old");
        fs::rename(&path, &old).expect("rotate");
        fs::write(&path, SHORTCUTS).expect("the rotated log starts small");
        let (percent, stage) = tailer.snapshot(Duration::from_secs(2));
        assert_eq!(stage, Some(InstallStage::Finalizing));
        assert_eq!(
            percent,
            Some(94),
            "the unread tail of .old must still count"
        );
    }

    #[test]
    fn finalizing_keeps_the_percentage_for_two_seconds_then_pulses() {
        let dir = tempfile::tempdir().expect("temp");
        let (mut tailer, path) = tailer(dir.path());
        fs::write(&path, format!("{HEADER}{HEADER_TAIL}{FIRST}{BACKING_UP}")).expect("write");
        assert_eq!(
            tailer.snapshot(Duration::from_secs(2)),
            (Some(94), Some(InstallStage::Finalizing))
        );
        assert_eq!(
            tailer.snapshot(Duration::from_secs(3)).0,
            Some(94),
            "one idle second is still inside the grace"
        );
        let (idle, stage) = tailer.snapshot(Duration::from_secs(4));
        assert_eq!(idle, None, "a hung cleanup must pulse, never hold at 94");
        assert_eq!(stage, Some(InstallStage::Finalizing));
    }

    #[test]
    fn no_handle_survives_a_poll() {
        let dir = tempfile::tempdir().expect("temp");
        let (mut tailer, path) = tailer(dir.path());
        fs::write(&path, format!("{HEADER}{HEADER_TAIL}{FIRST}{BACKING_UP}")).expect("write");
        tailer.snapshot(Duration::from_secs(2));
        fs::remove_file(&path).expect("the child's log must not be held open");
        assert_eq!(
            tailer.snapshot(Duration::from_secs(2)),
            (Some(94), Some(InstallStage::Finalizing))
        );
    }
}
