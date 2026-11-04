use super::LeakScanner;

/// The data-safety verdict as fixed words plus a count. The words are not user data, so a
/// protected name that matches them cannot redact the verdict away.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyVerdict {
    Pass,
    /// The folder changed or a side could not be fully walked; `entries` counts the changes.
    Fail {
        entries: u64,
    },
    /// A check before or after did not finish.
    Unfinished,
    NotMeasured,
}

impl SafetyVerdict {
    pub const fn failed(self) -> bool {
        matches!(self, Self::Fail { .. } | Self::Unfinished)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "DATA SAFETY: PASS - your folder did not change",
            Self::Fail { .. } => {
                "DATA SAFETY: FAIL - your folder changed or could not be fully checked; see the report"
            }
            Self::Unfinished => {
                "DATA SAFETY: FAIL - the folder check before or after did not finish; nothing is proved"
            }
            Self::NotMeasured => "DATA SAFETY: not checked - no folder chosen",
        }
    }

    /// The full line with its count when the scanner finds nothing in it as a whole; otherwise
    /// the bare constant label, which is public text.
    pub(super) fn line(self, scanner: &LeakScanner) -> Vec<u8> {
        let label = self.label().as_bytes();
        let mut line = label.to_vec();
        if let Self::Fail { entries } = self {
            line.extend_from_slice(format!(" (changed entries: {entries})").as_bytes());
            if scanner.count_leaks(&line) != 0 {
                line = label.to_vec();
            }
        }
        line.push(b'\n');
        line
    }
}

#[cfg(test)]
mod tests {
    use super::{LeakScanner, SafetyVerdict};

    const LABEL: &str =
        "DATA SAFETY: FAIL - your folder changed or could not be fully checked; see the report\n";

    fn line(needle: &str, entries: u64) -> String {
        let scanner = LeakScanner::new(&[needle]).unwrap();
        String::from_utf8(SafetyVerdict::Fail { entries }.line(&scanner)).unwrap()
    }

    #[test]
    fn a_needle_spanning_the_count_and_fixed_text_drops_the_count() {
        for (needle, entries) in [("1)", 1), (": 1", 1), ("s: 4", 4), ("2", 2), ("12", 12)] {
            assert_eq!(line(needle, entries), LABEL, "needle {needle:?}");
        }
    }

    #[test]
    fn a_needle_in_the_fixed_words_keeps_the_label_and_drops_the_count() {
        assert_eq!(line("folder", 7), LABEL);
        assert_eq!(line("entries", 7), LABEL);
    }

    #[test]
    fn a_clean_count_is_shown() {
        let shown = line("zqxjv", 3);
        assert_eq!(
            shown,
            format!("{} (changed entries: 3)\n", LABEL.trim_end())
        );
    }
}
