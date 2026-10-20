use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};

pub const DIFF_BUDGET: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "This file mixes line endings and the change is too large to keep every line's ending. Nothing was saved."
)]
pub struct TooLarge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEnds {
    hashes: Vec<u64>,
    crlf: Vec<bool>,
    default_crlf: bool,
    stray_cr: bool,
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

fn majority_crlf(crlf: &[bool]) -> bool {
    crlf.iter().filter(|is_crlf| **is_crlf).count() * 2 > crlf.len()
}

fn break_text(is_crlf: bool) -> &'static str {
    if is_crlf { "\r\n" } else { "\n" }
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
        let mut stray_cr = false;
        let mut rest = text;
        while let Some(at) = rest.find('\n') {
            let line = &rest[..at];
            let (body, is_crlf) = match line.strip_suffix('\r') {
                Some(body) => (body, true),
                None => (line, false),
            };
            stray_cr |= body.ends_with('\r');
            unified.push_str(body);
            unified.push('\n');
            hashes.push(hash_of(body));
            crlf.push(is_crlf);
            rest = &rest[at + 1..];
        }
        stray_cr |= rest.ends_with('\r');
        unified.push_str(rest);
        hashes.push(hash_of(rest));
        let default_crlf = majority_crlf(&crlf);
        let ends = Self {
            hashes,
            crlf,
            default_crlf,
            stray_cr,
        };
        Some((unified, ends))
    }

    #[must_use]
    pub fn has_stray_cr(&self) -> bool {
        self.stray_cr
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

    pub fn apply(&self, edited: &str) -> Result<(String, Self), TooLarge> {
        self.apply_within(edited, DIFF_BUDGET)
    }

    pub fn apply_within(&self, edited: &str, budget: Duration) -> Result<(String, Self), TooLarge> {
        let lines = self.lines_of(edited);
        let hashes: Vec<u64> = lines.iter().map(|line| hash_of(line)).collect();
        let origins = self.origins(&hashes, budget)?;
        let crlf: Vec<bool> = origins
            .iter()
            .take(lines.len() - 1)
            .map(|origin| {
                origin
                    .and_then(|old| self.crlf.get(old).copied())
                    .unwrap_or(self.default_crlf)
            })
            .collect();
        let mut text = String::with_capacity(edited.len() + lines.len());
        for (at, line) in lines.iter().enumerate() {
            text.push_str(line);
            if let Some(is_crlf) = crlf.get(at) {
                text.push_str(break_text(*is_crlf));
            }
        }
        let default_crlf = majority_crlf(&crlf);
        let ends = Self {
            hashes,
            crlf,
            default_crlf,
            stray_cr: false,
        };
        Ok((text, ends))
    }

    fn lines_of<'a>(&self, edited: &'a str) -> Vec<&'a str> {
        let mut lines: Vec<&str> = edited.split('\n').collect();
        if !self.stray_cr {
            let last = lines.len() - 1;
            for line in &mut lines[..last] {
                *line = line.strip_suffix('\r').unwrap_or(line);
            }
        }
        lines
    }

    fn origins(&self, new: &[u64], budget: Duration) -> Result<Vec<Option<usize>>, TooLarge> {
        let old_rev: Vec<u64> = self.hashes.iter().rev().copied().collect();
        let new_rev: Vec<u64> = new.iter().rev().copied().collect();
        let deadline = Instant::now() + budget;
        let ops =
            capture_diff_slices_deadline(Algorithm::Patience, &old_rev, &new_rev, Some(deadline));
        if Instant::now() >= deadline {
            return Err(TooLarge);
        }
        let mut origins = vec![None; new.len()];
        for op in ops {
            let (tag, old, changed) = op.as_tag_tuple();
            if !matches!(tag, DiffTag::Equal | DiffTag::Replace) {
                continue;
            }
            let old_start = self.hashes.len() - old.end;
            let new_start = new.len() - changed.end;
            for step in 0..old.len().min(changed.len()) {
                origins[new_start + step] = Some(old_start + step);
            }
        }
        Ok(origins)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{DIFF_BUDGET, LineEnds, TooLarge};

    const MIXED: &str = "one\r\ntwo\nthree\r\nfour\nfive";

    fn applied(ends: &LineEnds, edited: &str) -> String {
        ends.apply(edited).expect("within the budget").0
    }

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
        assert_eq!(applied(&ends, &text), MIXED);
    }

    #[test]
    fn editing_one_line_keeps_every_other_ending() {
        let ends = ends_of(MIXED);
        let edited = "one\ntwo\nTHREE\nfour\nfive";
        assert_eq!(applied(&ends, edited), "one\r\ntwo\nTHREE\r\nfour\nfive");
    }

    #[test]
    fn inserting_a_line_keeps_the_endings_around_it() {
        let ends = ends_of(MIXED);
        let edited = "one\ntwo\nthree\nnew\nfour\nfive";
        assert_eq!(
            applied(&ends, edited),
            "one\r\ntwo\nthree\r\nnew\nfour\nfive"
        );
    }

    #[test]
    fn deleting_a_line_keeps_the_endings_around_it() {
        let ends = ends_of(MIXED);
        assert_eq!(
            applied(&ends, "one\nthree\nfour\nfive"),
            "one\r\nthree\r\nfour\nfive"
        );
    }

    #[test]
    fn appending_lines_uses_the_majority_style() {
        let ends = ends_of("a\r\nb\r\nc\nd\r\ne");
        assert_eq!(
            applied(&ends, "a\nb\nc\nd\ne\nf\n"),
            "a\r\nb\r\nc\nd\r\ne\r\nf\r\n"
        );
    }

    #[test]
    fn crlf_marks_blank_the_plain_line_feeds() {
        let ends = ends_of("a\r\nb\nc\r\nd");
        assert_eq!(ends.crlf_marks("a\nb\nc\nd"), "a\nb c\nd");
    }

    fn ending_of(at: usize) -> &'static str {
        if at.is_multiple_of(3) { "\r\n" } else { "\n" }
    }

    fn document(lines: &[(String, &'static str)]) -> String {
        lines
            .iter()
            .map(|(line, ending)| format!("{line}{ending}"))
            .collect()
    }

    #[test]
    fn two_edits_far_apart_leave_every_line_between_them_alone() {
        let original: Vec<(String, &str)> = (0..2000)
            .map(|at| (format!("line {at}"), ending_of(at)))
            .collect();
        let (text, ends) = LineEnds::unify(&document(&original)).expect("mixed");
        let mut edited = original.clone();
        edited[1005].0 = "changed 1005".to_owned();
        edited.insert(6, ("added after 5".to_owned(), "\n"));
        let plain: String = edited.iter().map(|(line, _)| format!("{line}\n")).collect();
        assert_eq!(text.lines().count(), 2000);
        assert_eq!(applied(&ends, &plain), document(&edited));
    }

    #[test]
    fn known_limit_deleting_either_of_two_equal_lines_keeps_the_first_ones_ending() {
        let ends = ends_of("x\r\nx\nz");
        assert_eq!(applied(&ends, "x\nz"), "x\nz");
    }

    #[test]
    fn a_changed_line_keeps_the_ending_of_the_line_it_replaces() {
        let ends = ends_of("a\r\nb\nc\r\nd\ne");
        assert_eq!(applied(&ends, "a\nB\nC\nd\ne"), "a\r\nB\nC\r\nd\ne");
    }

    #[test]
    fn a_line_body_that_ends_in_a_lone_cr_is_flagged_and_kept() {
        let (text, ends) = LineEnds::unify("a\r\r\nb\nc").expect("mixed");
        assert!(ends.has_stray_cr());
        assert_eq!(applied(&ends, &text), "a\r\r\nb\nc");
        let (_, plain) = LineEnds::unify("a\rb\r\nc\nd").expect("mixed");
        assert!(!plain.has_stray_cr());
    }

    #[test]
    fn the_table_from_a_save_diffs_the_next_save_against_the_saved_text() {
        let ends = ends_of(MIXED);
        let first = "new\none\ntwo\nthree\nfour\nfive";
        let (saved, table) = ends.apply(first).expect("within the budget");
        let second = "new\none\ntwo\nTHREE\nfour\nfive";
        assert_eq!(applied(&table, second), saved.replace("three", "THREE"));
    }

    #[test]
    fn the_table_from_a_save_equals_the_table_of_the_text_it_wrote() {
        let ends = ends_of(MIXED);
        let (saved, table) = ends
            .apply("new\none\ntwo\nTHREE\nfour\nfive")
            .expect("within the budget");
        assert_eq!(saved, "new\none\r\ntwo\nTHREE\r\nfour\nfive");
        assert_eq!(LineEnds::unify(&saved).map(|(_, read)| read), Some(table));
    }

    #[test]
    fn an_expired_deadline_refuses_an_edit_to_a_mixed_file() {
        let ends = ends_of(MIXED);
        let edited = "one\ntwo\nTHREE\nfour\nfive";
        assert_eq!(ends.apply_within(edited, Duration::ZERO), Err(TooLarge));
        assert!(ends.apply_within(edited, DIFF_BUDGET).is_ok());
    }

    #[test]
    fn the_refusal_says_that_nothing_was_saved() {
        assert_eq!(
            TooLarge.to_string(),
            "This file mixes line endings and the change is too large to keep every line's ending. Nothing was saved."
        );
    }
}
