use super::Quotes;
use crate::redact::names::replace;
use crate::redact::{DriveKind, PathKind, Redactor};

struct Location<'a> {
    head: &'a str,
    folder: &'a str,
    file: &'a str,
    tail: &'a str,
}

impl Redactor {
    #[must_use]
    pub fn backtrace(&self, text: &str) -> String {
        text.split_inclusive('\n')
            .map(|line| match source_location(line) {
                Some(location) => self.location(&location),
                None => self.redact(line, Quotes::Code),
            })
            .collect()
    }

    fn location(&self, location: &Location<'_>) -> String {
        let folder = match location.folder.char_indices().next_back() {
            Some((at, separator)) => {
                let folder = &location.folder[..at];
                let folder = self.path(folder, PathKind::Folder, DriveKind::Unknown);
                format!("{}{separator}", folder.as_str())
            }
            None => String::new(),
        };
        let file = replace(location.file, &self.replacements());
        format!("{}{folder}{file}{}", location.head, location.tail)
    }
}

fn source_location(line: &str) -> Option<Location<'_>> {
    let body = line.trim_start();
    let head_len = line.len() - body.len() + "at ".len();
    let rest = body.strip_prefix("at ")?;
    let mut path = rest.trim_end();
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((start, number)) if is_number(number) => path = start,
            _ => break,
        }
    }
    if !path.ends_with(".rs") || path.len() == rest.trim_end().len() {
        return None;
    }
    let file_start = path.rfind(['/', '\\']).map_or(0, |at| at + 1);
    Some(Location {
        head: &line[..head_len],
        folder: &path[..file_start],
        file: &path[file_start..],
        tail: &rest[path.len()..],
    })
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}
