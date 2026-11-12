//! Velopack 1.2.161 log lines turned into a byte-weighted install percentage (IP1, IP2).

use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

/// The phase the log has reached; the UI pulses whenever no percentage is verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallStage {
    Preparing,
    CopyingPackage,
    Extracting,
    Finalizing,
}

/// The nupkg's own length and the uncompressed size of every `lib/` entry Velopack writes.
#[derive(Debug, Default)]
pub struct PackageWeights {
    compressed: u64,
    lib_bytes: u64,
    entries: BTreeMap<String, u64>,
}

impl PackageWeights {
    /// Reads the embedded nupkg's zip directory; a damaged archive returns an error.
    pub fn from_nupkg(bytes: &[u8]) -> io::Result<Self> {
        let mut zip = zip::ZipArchive::new(io::Cursor::new(bytes))
            .map_err(|_| io::Error::other("the nupkg is not a readable zip"))?;
        let mut weights = Self {
            compressed: u64::try_from(bytes.len()).map_err(io::Error::other)?,
            ..Self::default()
        };
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|_| io::Error::other("a nupkg entry is damaged"))?;
            let name = entry
                .name()
                .map_err(|_| io::Error::other("a nupkg entry name is not UTF-8"))?
                .into_owned();
            let Some(inner) = lib_key(&name) else {
                continue;
            };
            if name.ends_with("Squirrel.exe") || inner.ends_with("_ExecutionStub.exe") {
                continue;
            }
            weights.lib_bytes = weights.lib_bytes.saturating_add(entry.size());
            weights.entries.insert(name, entry.size());
        }
        Ok(weights)
    }
}

/// The path below `lib/<target>/`, or `None` for the entries Velopack never extracts.
fn lib_key(name: &str) -> Option<&str> {
    let rest = name
        .strip_prefix("lib/")
        .or_else(|| name.strip_prefix("lib\\"))?;
    let (_, inner) = rest.split_once(['/', '\\'])?;
    let directory = inner.ends_with('/') || inner.ends_with('\\');
    (!inner.is_empty() && !directory).then_some(inner)
}

/// One entry line's zip key and whether Velopack writes that entry to disk.
fn entry_line(line: &str) -> Option<(String, bool)> {
    let (head, rest) = line.split_once('\'')?;
    let key = rest.split_once('\'')?.0.to_owned();
    let head = head.trim_end();
    if head.ends_with("Extracting") {
        return Some((key, true));
    }
    let skipped = head.ends_with("Skipped") || head.ends_with("Skipped Stub (obsolete)");
    skipped.then_some((key, false))
}

/// Reads `Extracting {N} app files to ...`, the line that ends the package copy.
fn declared_files(line: &str) -> Option<usize> {
    let (head, _) = line.split_once(" app files")?;
    head.rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// The post-extract milestones: hooks, shortcuts and the two folder-swap steps.
fn finalizing(line: &str) -> bool {
    const MARKS: [&str; 5] = [
        "Creating shortcuts",
        "Starting process install hook",
        "Skipping --veloapp-obsolete hook.",
        "Backing up current dir",
        "Replacing current dir",
    ];
    MARKS.iter().any(|mark| line.contains(mark))
}

/// The fixed percentage of a folder-swap milestone, if the line carries one.
fn swap_percent(line: &str) -> Option<u8> {
    if line.contains("Backing up current dir") {
        Some(94)
    } else if line.contains("Replacing current dir") {
        Some(96)
    } else {
        None
    }
}

/// Accumulates verified work from the log; every percentage is monotonic and never 100.
pub struct Estimator {
    upgrade: bool,
    weights: Option<PackageWeights>,
    stage: InstallStage,
    declared: Option<usize>,
    total: u64,
    completed_bytes: u64,
    completed_entries: usize,
    by_bytes: bool,
    pending: Option<(String, bool)>,
    shown: Option<u8>,
    advanced_at: Duration,
}

impl Estimator {
    /// A fresh install also copies the nupkg; an upgrade has already staged it.
    pub fn new(upgrade: bool, weights: Option<PackageWeights>) -> Self {
        let by_bytes = weights.as_ref().is_some_and(|w| w.lib_bytes > 0);
        Self {
            upgrade,
            weights,
            stage: InstallStage::Preparing,
            declared: None,
            total: 0,
            completed_bytes: 0,
            completed_entries: 0,
            by_bytes,
            pending: None,
            shown: None,
            advanced_at: Duration::ZERO,
        }
    }

    /// Feeds one complete log line; unrecognized lines change nothing.
    pub fn feed_line(&mut self, line: &str, now: Duration) {
        if self.absorb(line) {
            self.advanced_at = now;
            self.remember();
        }
    }

    /// The verified percentage, or `None` while the bar must pulse.
    pub fn percent(&self) -> Option<u8> {
        let extracting = matches!(
            self.stage,
            InstallStage::Extracting | InstallStage::Finalizing
        );
        if !extracting || self.declared == Some(0) {
            return None;
        }
        self.shown
    }

    /// The phase once a milestone named it, and `None` before the first one.
    pub fn stage(&self) -> Option<InstallStage> {
        (self.stage != InstallStage::Preparing).then_some(self.stage)
    }

    /// When a recognized line last advanced the estimate, for the stall guard.
    pub fn advanced_at(&self) -> Duration {
        self.advanced_at
    }

    /// Applies one recognized line; `false` leaves the estimate untouched.
    fn absorb(&mut self, line: &str) -> bool {
        if let Some(count) = declared_files(line) {
            self.credit_pending();
            self.declared = Some(count);
            self.stage = InstallStage::Extracting;
            self.total = self.total_bytes();
            if !self.upgrade {
                self.completed_bytes = self.compressed();
            }
            return true;
        }
        if let Some(entry) = entry_line(line) {
            self.credit_pending();
            self.pending = Some(entry);
            return true;
        }
        if line.contains("Creating symlink") {
            self.credit_pending();
            return true;
        }
        if line.contains("Copying nupkg to packages directory") {
            self.stage = InstallStage::CopyingPackage;
            return true;
        }
        if finalizing(line) {
            self.credit_pending();
            self.stage = InstallStage::Finalizing;
            if let Some(percent) = swap_percent(line) {
                self.raise(percent);
            }
            return true;
        }
        false
    }

    /// Credits the entry whose line came last: Velopack logs before it writes.
    fn credit_pending(&mut self) {
        let Some((key, written)) = self.pending.take() else {
            return;
        };
        self.completed_entries = self.completed_entries.saturating_add(1);
        if !written {
            return;
        }
        match self.weights.as_ref().and_then(|w| w.entries.get(&key)) {
            Some(size) => self.completed_bytes = self.completed_bytes.saturating_add(*size),
            None => self.by_bytes = false,
        }
    }

    /// Keeps the high-water mark, so a later switch to entry counting never lowers the bar.
    fn remember(&mut self) {
        let Some(scaled) = self.scaled() else {
            return;
        };
        if let Ok(percent) = u8::try_from(scaled.clamp(1, 92)) {
            self.raise(percent);
        }
    }

    /// Never lets the shown percentage fall.
    fn raise(&mut self, percent: u8) {
        self.shown = Some(self.shown.map_or(percent, |old| old.max(percent)));
    }

    /// The verified share of the work on the 1..=92 extract scale.
    fn scaled(&self) -> Option<u64> {
        if self.by_bytes {
            return (self.total > 0).then(|| self.completed_bytes.saturating_mul(92) / self.total);
        }
        let declared = u64::try_from(self.declared?).ok()?;
        let done = u64::try_from(self.completed_entries).ok()?;
        (declared > 0).then(|| done.saturating_mul(92) / declared)
    }

    /// A fresh install also copies the nupkg; an upgrade staged it before the child ran.
    fn total_bytes(&self) -> u64 {
        let lib = self.weights.as_ref().map_or(0, |w| w.lib_bytes);
        if self.upgrade {
            lib
        } else {
            lib.saturating_add(self.compressed())
        }
    }

    /// The nupkg's own length, the copy a fresh install completes before extracting.
    fn compressed(&self) -> u64 {
        self.weights.as_ref().map_or(0, |w| w.compressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A weight table with a 100-byte package and the given `lib/` entries.
    fn table(entries: &[(&str, u64)]) -> PackageWeights {
        PackageWeights {
            compressed: 100,
            lib_bytes: entries.iter().map(|(_, size)| size).sum(),
            entries: entries
                .iter()
                .map(|(key, size)| ((*key).to_owned(), *size))
                .collect(),
        }
    }

    /// One Velopack log line with the prefix its WriteLogger writes.
    fn log(message: &str) -> String {
        format!("[Setup:7] [10:00:00] [DEBUG] {message}")
    }

    /// Feeds whole lines at one second apart.
    fn feed(estimator: &mut Estimator, lines: &[&str]) {
        for (index, message) in lines.iter().enumerate() {
            let now = Duration::from_secs(index as u64 + 1);
            estimator.feed_line(&log(message), now);
        }
    }

    const HEADER: &str = "Extracting 3 app files to \"C:\\Navo\\current\"...";
    const FIRST: &str = "    0 Extracting 'lib/net8/navo.exe' to \"C:\\Navo\\current\\navo.exe\"";
    const SECOND: &str = "    1 Extracting 'lib/net8/data.bin' to \"C:\\Navo\\current\\data.bin\"";
    const THIRD: &str = "    2 Extracting 'lib/net8/readme.txt' to \"C:\\Navo\\current\\r.txt\"";

    #[test]
    fn a_fresh_install_stays_indeterminate_until_the_extract_header() {
        let mut estimator = Estimator::new(false, Some(table(&[("lib/net8/navo.exe", 300)])));
        feed(&mut estimator, &["Copying nupkg to packages directory..."]);
        assert_eq!(estimator.percent(), None);
        assert_eq!(estimator.stage(), Some(InstallStage::CopyingPackage));
        feed(&mut estimator, &[HEADER]);
        assert_eq!(estimator.stage(), Some(InstallStage::Extracting));
        assert!(estimator.percent().is_some(), "the copy is verified work");
    }

    #[test]
    fn an_entry_counts_only_once_the_next_line_confirms_the_write() {
        let weights = table(&[("lib/net8/navo.exe", 300), ("lib/net8/data.bin", 600)]);
        let mut estimator = Estimator::new(true, Some(weights));
        feed(&mut estimator, &[HEADER, FIRST]);
        let after_first = estimator.percent().expect("header seen");
        feed(&mut estimator, &[SECOND]);
        let after_second = estimator.percent().expect("header seen");
        assert!(
            after_second > after_first,
            "{after_first} -> {after_second}: the first entry's bytes must land"
        );
        assert_eq!(after_first, 1, "nothing is verified before the first write");
    }

    #[test]
    fn an_upgrade_weights_only_the_extracted_bytes() {
        let entries = [("lib/net8/navo.exe", 300), ("lib/net8/data.bin", 600)];
        let mut upgrade = Estimator::new(true, Some(table(&entries)));
        feed(&mut upgrade, &[HEADER, FIRST, SECOND]);
        let mut fresh = Estimator::new(false, Some(table(&entries)));
        feed(&mut fresh, &[HEADER, FIRST, SECOND]);
        assert_eq!(upgrade.percent(), Some(30), "300 of 900 extracted bytes");
        assert_eq!(
            fresh.percent(),
            Some(36),
            "400 of 1000 bytes, copy included"
        );
    }

    #[test]
    fn an_unmatched_key_falls_back_to_the_entry_count() {
        let mut estimator = Estimator::new(true, Some(table(&[("lib/net8/other.exe", 300)])));
        feed(&mut estimator, &[HEADER, FIRST, SECOND, THIRD]);
        assert_eq!(
            estimator.percent(),
            Some(61),
            "two of the three declared entries are confirmed"
        );
    }

    #[test]
    fn without_a_weight_table_the_entry_count_drives_the_bar() {
        let mut estimator = Estimator::new(true, None);
        feed(&mut estimator, &[HEADER, FIRST, SECOND, THIRD]);
        assert_eq!(estimator.percent(), Some(61));
    }

    #[test]
    fn a_declared_zero_file_count_never_shows_a_percentage() {
        let empty = "Extracting 0 app files to \"C:\\Navo\\current\"...";
        let mut weighed = Estimator::new(true, Some(table(&[("lib/net8/navo.exe", 300)])));
        feed(&mut weighed, &[empty]);
        assert_eq!(weighed.percent(), None);
        let mut unweighed = Estimator::new(true, None);
        feed(&mut unweighed, &[empty]);
        assert_eq!(unweighed.percent(), None);
    }

    #[test]
    fn the_percentage_never_falls_and_never_reaches_100() {
        let weights = table(&[("lib/net8/navo.exe", 300), ("lib/net8/data.bin", 600)]);
        let mut estimator = Estimator::new(true, Some(weights));
        let mut last = 0u8;
        for message in [HEADER, FIRST, SECOND, THIRD, "Creating shortcuts..."] {
            feed(&mut estimator, &[message]);
            let percent = estimator.percent().expect("a milestone was seen");
            assert!(percent >= last, "{percent} dropped below {last}");
            assert!(percent < 100, "{percent} claimed the child had finished");
            last = percent;
        }
    }

    #[test]
    fn the_folder_swap_milestones_raise_the_bar_to_94_and_96() {
        let mut estimator = Estimator::new(true, Some(table(&[("lib/net8/navo.exe", 300)])));
        feed(&mut estimator, &[HEADER, FIRST, "Backing up current dir"]);
        assert_eq!(estimator.percent(), Some(94));
        assert_eq!(estimator.stage(), Some(InstallStage::Finalizing));
        feed(&mut estimator, &["Replacing current dir"]);
        assert_eq!(estimator.percent(), Some(96));
    }

    #[test]
    fn unrecognized_lines_leave_the_bar_indeterminate() {
        let mut estimator = Estimator::new(true, Some(table(&[("lib/net8/navo.exe", 300)])));
        feed(
            &mut estimator,
            &[
                "Extracting zip file to path: \"C:\\Temp\"",
                "Extracting stub 'Squirrel.exe' to \"C:\\Navo\\Update.exe\"",
                "some future Velopack wording",
            ],
        );
        assert_eq!(estimator.percent(), None);
        assert_eq!(estimator.stage(), None);
    }

    #[test]
    fn only_a_recognized_line_moves_the_advance_clock() {
        let mut estimator = Estimator::new(true, None);
        estimator.feed_line(&log("some future Velopack wording"), Duration::from_secs(9));
        assert_eq!(estimator.advanced_at(), Duration::ZERO);
        estimator.feed_line(&log(HEADER), Duration::from_secs(4));
        assert_eq!(estimator.advanced_at(), Duration::from_secs(4));
    }

    #[test]
    fn from_nupkg_sums_only_the_lib_entries_velopack_writes() {
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("NavoApp.nuspec", "<package/>"),
            ("lib/net8/Squirrel.exe", "update"),
            ("lib/net8/navo.exe", "0123456789"),
            ("lib/net8/navo_ExecutionStub.exe", "stub"),
            ("lib/net8/sub/", ""),
            ("package/services/metadata/core-properties/1.psmdcp", "meta"),
        ] {
            zip.start_file(name, options).expect("start");
            io::Write::write_all(&mut zip, body.as_bytes()).expect("write");
        }
        let bytes = zip.finish().expect("finish").into_inner();
        let weights = PackageWeights::from_nupkg(&bytes).expect("readable zip");
        assert_eq!(weights.lib_bytes, 10);
        assert_eq!(weights.compressed, bytes.len() as u64);
        assert_eq!(
            weights.entries.keys().collect::<Vec<_>>(),
            ["lib/net8/navo.exe"]
        );
        assert!(PackageWeights::from_nupkg(b"not a zip").is_err());
    }
}
