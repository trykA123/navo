use super::names::replace;
use super::path::extension;
use super::{DriveKind, PathKind, Redactor};

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
        output
    }

    fn scan_paths<'a>(&self, mut text: &'a str) -> Vec<Piece<'a>> {
        let mut pieces = Vec::new();
        while let Some((start, _)) = text
            .char_indices()
            .find(|(index, _)| self.is_path_start(&text[*index..]))
        {
            pieces.push(Piece::Text(&text[..start]));
            let path = &text[start..];
            let end = self.path_end(path);
            let redacted = self.path(&path[..end], PathKind::File, DriveKind::Unknown);
            pieces.push(Piece::Path(redacted.to_string()));
            text = &path[end..];
        }
        pieces.push(Piece::Text(text));
        pieces
    }

    fn is_path_start(&self, text: &str) -> bool {
        if text.starts_with(r"\\") || drive_start(text) {
            return true;
        }
        self.identity
            .profile
            .as_deref()
            .filter(|profile| profile.starts_with('/'))
            .and_then(|profile| text.strip_prefix(profile))
            .is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|ch| ch == '/' || ch.is_whitespace() || hard_boundary(ch))
            })
    }

    fn path_end(&self, path: &str) -> usize {
        let end = path
            .char_indices()
            .find(|(index, ch)| {
                hard_boundary(*ch)
                    || (*ch == ' ' && self.is_path_start(&path[index + ch.len_utf8()..]))
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

fn drive_start(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('\\' | '/')) if letter.is_ascii_alphabetic()
    )
}

fn hard_boundary(ch: char) -> bool {
    matches!(ch, '\'' | '"' | '<' | '>' | '|' | '\t' | '\n' | '\r')
}

fn file_end(segment: &str) -> usize {
    let mut word_start = 0;
    for (index, ch) in segment.char_indices() {
        if !ch.is_whitespace() {
            continue;
        }
        if extension(&segment[word_start..index]).is_some() {
            return index;
        }
        word_start = index + ch.len_utf8();
    }
    segment.len()
}
