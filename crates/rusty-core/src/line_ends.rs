use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEnds {
    hashes: Vec<u64>,
    crlf: Vec<bool>,
    default_crlf: bool,
}

fn is_mixed(text: &str) -> bool {
    let bytes = text.as_bytes();
    let breaks = memchr::memchr_iter(b'\n', bytes).count();
    let crlf = memchr::memchr_iter(b'\n', bytes)
        .filter(|at| *at > 0 && bytes[at - 1] == b'\r')
        .count();
    crlf > 0 && crlf < breaks
}

fn hash_of(line: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    line.hash(&mut hasher);
    hasher.finish()
}

impl LineEnds {
    #[must_use]
    pub fn unify(text: &str) -> Option<(String, Self)> {
        if !is_mixed(text) {
            return None;
        }
        let mut unified = String::with_capacity(text.len());
        let mut hashes = Vec::new();
        let mut crlf = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find('\n') {
            let line = &rest[..at];
            let (body, is_crlf) = match line.strip_suffix('\r') {
                Some(body) => (body, true),
                None => (line, false),
            };
            unified.push_str(body);
            unified.push('\n');
            hashes.push(hash_of(body));
            crlf.push(is_crlf);
            rest = &rest[at + 1..];
        }
        unified.push_str(rest);
        hashes.push(hash_of(rest));
        let default_crlf = crlf.iter().filter(|is_crlf| **is_crlf).count() * 2 > crlf.len();
        let ends = Self {
            hashes,
            crlf,
            default_crlf,
        };
        Some((unified, ends))
    }

    #[must_use]
    pub fn crlf_marks(&self, text: &str) -> String {
        let mut marks = String::with_capacity(text.len());
        let mut breaks = self.crlf.iter();
        for character in text.chars() {
            if character == '\n' && breaks.next() == Some(&false) {
                marks.push(' ');
            } else {
                marks.push(character);
            }
        }
        marks
    }

    fn is_crlf_after(&self, shared: &Shared, after: usize) -> bool {
        let Shared {
            old,
            new,
            prefix,
            suffix,
        } = *shared;
        let from = if after < prefix {
            Some(after)
        } else if after >= new - suffix {
            Some(after + old - new)
        } else {
            (old == new).then_some(after)
        };
        from.and_then(|at| self.crlf.get(at).copied())
            .unwrap_or(self.default_crlf)
    }

    #[must_use]
    pub fn apply(&self, edited: &str) -> String {
        let edited = edited.replace("\r\n", "\n");
        let lines: Vec<&str> = edited.split('\n').collect();
        let hashes: Vec<u64> = lines.iter().map(|line| hash_of(line)).collect();
        let (old, new) = (self.hashes.len(), hashes.len());
        let room = old.min(new);
        let prefix = (0..room)
            .take_while(|at| self.hashes[*at] == hashes[*at])
            .count();
        let suffix = (0..room - prefix)
            .take_while(|back| self.hashes[old - 1 - back] == hashes[new - 1 - back])
            .count();
        let shared = Shared {
            old,
            new,
            prefix,
            suffix,
        };
        let mut text = String::with_capacity(edited.len() + lines.len());
        for (at, line) in lines.iter().enumerate() {
            text.push_str(line);
            if at + 1 < new {
                text.push_str(break_text(self.is_crlf_after(&shared, at)));
            }
        }
        text
    }
}

#[derive(Clone, Copy)]
struct Shared {
    old: usize,
    new: usize,
    prefix: usize,
    suffix: usize,
}

fn break_text(is_crlf: bool) -> &'static str {
    if is_crlf { "\r\n" } else { "\n" }
}

#[cfg(test)]
mod tests {
    use super::LineEnds;

    const MIXED: &str = "one\r\ntwo\nthree\r\nfour\nfive";

    fn ends_of(text: &str) -> LineEnds {
        LineEnds::unify(text).expect("mixed").1
    }

    #[test]
    fn a_file_with_one_style_is_not_mixed() {
        assert_eq!(LineEnds::unify("a\r\nb\r\n"), None);
        assert_eq!(LineEnds::unify("a\nb\n"), None);
        assert_eq!(LineEnds::unify("no breaks"), None);
    }

    #[test]
    fn unifying_a_mixed_file_gives_lf_text() {
        let (text, _) = LineEnds::unify(MIXED).expect("mixed");
        assert_eq!(text, "one\ntwo\nthree\nfour\nfive");
    }

    #[test]
    fn applying_the_unedited_text_gives_the_original_back() {
        let (text, ends) = LineEnds::unify(MIXED).expect("mixed");
        assert_eq!(ends.apply(&text), MIXED);
    }

    #[test]
    fn editing_one_line_keeps_every_other_ending() {
        let ends = ends_of(MIXED);
        let edited = "one\ntwo\nTHREE\nfour\nfive";
        assert_eq!(ends.apply(edited), "one\r\ntwo\nTHREE\r\nfour\nfive");
    }

    #[test]
    fn inserting_a_line_keeps_the_endings_around_it() {
        let ends = ends_of(MIXED);
        let edited = "one\ntwo\nthree\nnew\nfour\nfive";
        assert_eq!(ends.apply(edited), "one\r\ntwo\nthree\r\nnew\nfour\nfive");
    }

    #[test]
    fn deleting_a_line_keeps_the_endings_around_it() {
        let ends = ends_of(MIXED);
        assert_eq!(
            ends.apply("one\nthree\nfour\nfive"),
            "one\r\nthree\r\nfour\nfive"
        );
    }

    #[test]
    fn appending_lines_uses_the_majority_style() {
        let ends = ends_of("a\r\nb\r\nc\nd\r\ne");
        assert_eq!(
            ends.apply("a\nb\nc\nd\ne\nf\n"),
            "a\r\nb\r\nc\nd\r\ne\r\nf\r\n"
        );
    }

    #[test]
    fn crlf_marks_blank_the_plain_line_feeds() {
        let ends = ends_of("a\r\nb\nc\r\nd");
        assert_eq!(ends.crlf_marks("a\nb\nc\nd"), "a\nb c\nd");
    }
}
