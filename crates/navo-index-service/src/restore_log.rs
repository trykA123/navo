//! Formats the snapshot restore log line that the worker writes after loading a snapshot.

use std::time::Duration;

/// Formats the restored line for `letter` and `entries`, with load time `took` in whole ms.
pub(super) fn restored_line(letter: char, entries: usize, took: Duration) -> String {
    format!(
        "snapshot restored {letter}: entries={entries} ms={}",
        took.as_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restored_line_names_the_volume_entries_and_whole_milliseconds() {
        assert_eq!(
            restored_line('C', 5, Duration::from_millis(1234)),
            "snapshot restored C: entries=5 ms=1234"
        );
    }
}
