//! Windows command-line quoting for the crash server, kept platform-neutral so its tests run
//! everywhere.

use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

/// Encodes `arg` as UTF-16; Windows keeps its exact wide form, other platforms use lossy text.
fn encode(arg: &OsStr) -> Vec<u16> {
    #[cfg(windows)]
    {
        arg.encode_wide().collect()
    }
    #[cfg(not(windows))]
    {
        arg.to_string_lossy().encode_utf16().collect()
    }
}

/// Appends `arg` to a command line, quoted and escaped by the `CreateProcessW` rules.
pub(super) fn quote_arg(line: &mut Vec<u16>, arg: &OsStr) {
    let units = encode(arg);
    let needs_quotes = units.is_empty()
        || units.iter().any(|&unit| {
            unit == u16::from(b' ') || unit == u16::from(b'\t') || unit == u16::from(b'"')
        });
    if !needs_quotes {
        line.extend_from_slice(&units);
        line.push(u16::from(b' '));
        return;
    }
    line.push(u16::from(b'"'));
    let mut backslashes = 0;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
            continue;
        }
        // Backslashes only need doubling before a quote or the closing quote.
        let escape = if unit == u16::from(b'"') {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        line.extend(std::iter::repeat_n(u16::from(b'\\'), escape));
        line.push(unit);
        backslashes = 0;
    }
    line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    line.push(u16::from(b'"'));
    line.push(u16::from(b' '));
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::quote_arg;

    /// Quotes `arg` and returns the result without its trailing argument separator.
    fn quoted(arg: &str) -> String {
        let mut line = Vec::new();
        quote_arg(&mut line, OsStr::new(arg));
        assert_eq!(
            line.pop(),
            Some(u16::from(b' ')),
            "one argument ends with one space"
        );
        String::from_utf16(&line).expect("the quoted argument stays valid UTF-16")
    }

    #[test]
    fn a_plain_argument_is_not_quoted() {
        assert_eq!(quoted("abc"), "abc");
    }

    #[test]
    fn an_empty_argument_is_two_quotes() {
        assert_eq!(quoted(""), "\"\"");
    }

    #[test]
    fn spaces_and_tabs_are_quoted() {
        assert_eq!(quoted("a b"), "\"a b\"");
        assert_eq!(quoted("a\tb"), "\"a\tb\"");
    }

    #[test]
    fn an_embedded_quote_is_escaped() {
        assert_eq!(quoted("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn trailing_backslashes_are_doubled_when_the_argument_is_quoted() {
        assert_eq!(quoted("a b\\"), "\"a b\\\\\"");
    }

    #[test]
    fn a_bare_trailing_backslash_needs_no_quotes() {
        assert_eq!(quoted("a\\"), "a\\");
    }

    #[test]
    fn backslashes_before_a_quote_are_doubled_and_the_quote_escaped() {
        assert_eq!(quoted("a\\\"b"), "\"a\\\\\\\"b\"");
    }
}
