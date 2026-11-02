use super::names::replace;
use super::path::extension;
use super::{DriveKind, PathKind, Redactor};

mod backtrace;
mod quoted;

use quoted::{Quotes, hide_quoted};

enum Piece<'a> {
    Text(&'a str),
    Path(String),
}

impl Redactor {
    #[must_use]
    pub fn text(&self, text: &str) -> String {
        self.redact(text, Quotes::Prose)
    }

    fn redact(&self, text: &str, quotes: Quotes) -> String {
        let pieces = self.scan_paths(text);
        let replacements = self.replacements();
        let mut output = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => output.push_str(&replace(text, &replacements)),
                Piece::Path(path) => output.push_str(&path),
            }
        }
        hide_quoted(&output, &replacements, quotes)
    }

    fn scan_paths<'a>(&self, mut text: &'a str) -> Vec<Piece<'a>> {
        let names: Vec<String> = self
            .replacements()
            .iter()
            .map(|r| r.name().to_owned())
            .collect();
        let mut pieces = Vec::new();
        let mut before = None;
        while let Some(start) = self.find_path_start(text, before, &names) {
            let opened_by = text[..start].chars().next_back().or(before);
            pieces.push(Piece::Text(&text[..start]));
            let path = &text[start..];
            let end = self.path_end(path, opened_by);
            let redacted = self.path(&path[..end], PathKind::File, DriveKind::Unknown);
            pieces.push(Piece::Path(redacted.to_string()));
            before = path[..end].chars().next_back();
            text = &path[end..];
        }
        pieces.push(Piece::Text(text));
        pieces
    }

    fn find_path_start(
        &self,
        text: &str,
        mut before: Option<char>,
        names: &[String],
    ) -> Option<usize> {
        for (index, ch) in text.char_indices() {
            let rest = &text[index..];
            if self.is_path_start(rest, before)
                || (drive_start(rest) && ends_with_name(&text[..index], names))
                || url_path_start(&text[..index], rest)
            {
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
        if drive_start(text) && drive_follows(text, before) {
            return true;
        }
        if before == Some(':') && text.starts_with('/') && !text.starts_with("//") {
            return has_second_separator(text);
        }
        if !before.is_none_or(opens_path) {
            return false;
        }
        if ["~/", "./", "../"]
            .iter()
            .any(|start| text.starts_with(start))
        {
            return true;
        }
        if let Some(after) = text.strip_prefix("//") {
            return after
                .chars()
                .next()
                .is_some_and(|next| !next.is_whitespace() && !hard_boundary(next));
        }
        text.strip_prefix('/')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|next| !next.is_whitespace() && !hard_boundary(next))
    }

    fn path_end(&self, path: &str, opened_by: Option<char>) -> usize {
        let end = path
            .char_indices()
            .find(|(index, ch)| {
                (*ch != '\'' && hard_boundary(*ch))
                    || (*ch == ' ' && self.is_path_start(&path[index + 1..], Some(' ')))
            })
            .map_or(path.len(), |(index, _)| index);
        let end = match (opened_by, path[..end].rfind('\'')) {
            (Some('\''), Some(quote)) if !names_a_file(&path[quote + 1..end]) => quote,
            _ => end,
        };
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

fn drive_follows(text: &str, before: Option<char>) -> bool {
    match before {
        Some(ch) if ch.is_alphanumeric() => {
            ch.is_ascii_digit()
                || text.starts_with(|letter: char| letter.is_ascii_uppercase())
                || text.as_bytes().get(2) == Some(&b'\\')
                || (!text[2..].starts_with("//") && has_second_separator(&text[2..]))
        }
        _ => true,
    }
}

fn has_second_separator(text: &str) -> bool {
    text.chars()
        .skip(1)
        .take_while(|ch| !ch.is_whitespace() && !hard_boundary(*ch))
        .any(|ch| matches!(ch, '/' | '\\'))
}

fn names_a_file(tail: &str) -> bool {
    !tail.contains(['/', '\\'])
        && tail
            .split_whitespace()
            .any(|word| extension(word).is_some())
}

fn ends_with_name(text: &str, names: &[String]) -> bool {
    names.iter().any(|name| {
        let length = name.chars().count() + 2;
        let start = text
            .char_indices()
            .rev()
            .nth(length.saturating_sub(1))
            .map_or(0, |(index, _)| index);
        text[start..].to_lowercase().ends_with(name.as_str())
    })
}

fn url_path_start(before: &str, rest: &str) -> bool {
    rest.starts_with("//")
        && ["file:", "smb:"].iter().any(|scheme| {
            before
                .len()
                .checked_sub(scheme.len())
                .and_then(|start| before.get(start..))
                .is_some_and(|tail| tail.eq_ignore_ascii_case(scheme))
        })
}

fn drive_start(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('\\' | '/')) if letter.is_ascii_alphabetic()
    )
}

fn opens_path(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '=' | '\''
                | '"'
                | '('
                | '['
                | '{'
                | ','
                | ';'
                | '`'
                | '>'
                | '|'
                | '\u{2018}'
                | '\u{201c}'
        )
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
