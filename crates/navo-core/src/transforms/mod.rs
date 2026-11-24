//! Pure line transforms and case conversion for the F3 editor.
//! Every function takes text and returns text, keeping the input's line ending.

pub mod case;
pub mod codec;
mod invisible;
pub mod lines;

pub use case::{TextCase, convert_case};
pub use codec::{
    CodecError, Invisible, InvisibleKind, base64_decode, base64_encode, json_escape, json_unescape,
    scan_invisible, url_decode, url_encode,
};
pub use lines::{SortKind, SortSpec, dedup_lines, join_lines, sort_lines, trim_trailing};

/// Splits text into lines and reports the line ending and whether the text ended with one.
pub(crate) struct Lines<'a> {
    pub lines: Vec<&'a str>,
    pub ending: &'static str,
    pub final_ending: bool,
}

/// Splits `text` on CRLF when present, otherwise on LF; empty text has no lines.
pub(crate) fn split_lines(text: &str) -> Lines<'_> {
    let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let final_ending = text.ends_with(ending);
    let body = if final_ending {
        &text[..text.len() - ending.len()]
    } else {
        text
    };
    let lines = if text.is_empty() {
        Vec::new()
    } else {
        body.split(ending).collect()
    };
    Lines {
        lines,
        ending,
        final_ending,
    }
}

impl Lines<'_> {
    /// Joins `lines` with this text's ending, adding the final ending only when the input had one.
    pub(crate) fn join(&self, lines: &[&str]) -> String {
        let mut out = lines.join(self.ending);
        if self.final_ending {
            out.push_str(self.ending);
        }
        out
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod codec_tests;
