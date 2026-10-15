use super::names::replace;
use super::path::extension;
use super::{DriveKind, PathKind, Redactor};

mod quoted;

use quoted::hide_quoted;

enum Piece<'a> {
    Text(&'a str),
    Path(String),
}

impl Redactor {
    #[must_use]
    pub fn text(&self, text: &str) -> String {
        let pieces = self.scan_paths(text);
        let replacements = self.replacements();
        let mut output = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => output.push_str(&replace(text, &replacements)),
                Piece::Path(path) => output.push_str(&path),
            }
        }
        hide_quoted(&output, &replacements)
    }

    fn scan_paths<'a>(&self, mut text: &'a str) -> Vec<Piece<'a>> {
        let mut pieces = Vec::new();
        let mut before = None;
        while let Some(start) = self.find_path_start(text, before) {
            pieces.push(Piece::Text(&text[..start]));
            let path = &text[start..];
            let end = self.path_end(path);
            let redacted = self.path(&path[..end], PathKind::File, DriveKind::Unknown);
            pieces.push(Piece::Path(redacted.to_string()));
            before = path[..end].chars().next_back();
            text = &path[end..];
        }
        pieces.push(Piece::Text(text));
        pieces
    }

    fn find_path_start(&self, text: &str, mut before: Option<char>) -> Option<usize> {
        for (index, ch) in text.char_indices() {
            if self.is_path_start(&text[index..], before) {
                return Some(index);
            }
            before = Some(ch);
        }
        None
    }

    fn is_path_start(&self, text: &str, before: Option<char>) -> bool {
        if text.starts_with(r"\\") {
            return true;
        }
        let boundary = before.is_none_or(|ch| !ch.is_alphanumeric());
        if drive_start(text) && (boundary || text.as_bytes().get(2) == Some(&b'\\')) {
            return true;
        }
        if !before.is_none_or(opens_path) {
            return false;
        }
        if let Some(after) = text.strip_prefix("//") {
            return after
                .chars()
                .next()
                .is_some_and(|next| !next.is_whitespace() && next != '/' && !hard_boundary(next));
        }
        text.strip_prefix('/')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|next| !next.is_whitespace() && !hard_boundary(next))
    }

    fn path_end(&self, path: &str) -> usize {
        let end = path
            .char_indices()
            .find(|(index, ch)| {
                (hard_boundary(*ch) && !inner_apostrophe(path, *index, *ch))
                    || (*ch == ' ' && self.is_path_start(&path[index + 1..], Some(' ')))
            })
            .map_or(path.len(), |(index, _)| index);
        let run = path[..end].trim_end_matches(' ');
        let segment_start = run
            .char_indices()
            .filter(|(_, ch)| matches!(ch, '\\' | '/'))
            .map(|(index, ch)| index + ch.len_utf8())
            .next_back()
            .unwrap_or(0);
        segment_start + file_end(&run[segment_start..])
    }
}

fn inner_apostrophe(text: &str, index: usize, ch: char) -> bool {
    ch == '\''
        && text[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        && text[index + 1..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
}

fn drive_start(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('\\' | '/')) if letter.is_ascii_alphabetic()
    )
}

fn opens_path(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '=' | '\'' | '"' | '(' | '[' | '{' | ',' | ';' | '`')
}

fn hard_boundary(ch: char) -> bool {
    matches!(ch, '\'' | '"' | '<' | '>' | '|' | '\t' | '\n' | '\r')
}

fn file_end(segment: &str) -> usize {
    let mut word_start = 0;
    let mut end = None;
    for (index, ch) in segment.char_indices() {
        if !ch.is_whitespace() {
            continue;
        }
        if extension(&segment[word_start..index]).is_some() {
            end = Some(index);
        }
        word_start = index + ch.len_utf8();
    }
    if extension(&segment[word_start..]).is_some() {
        return segment.len();
    }
    end.unwrap_or(segment.len())
}
