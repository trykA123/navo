use super::anchor::{self, Anchor};
use super::scan::{self, BUCKETS};
use super::text::diff_text;

pub fn quality(old: &str, new: &str) -> Result<(u64, u64), String> {
    let old_lines = lines(old);
    let new_lines = lines(new);
    let mut old_candidates = scan::scan_bytes(old.as_bytes(), BUCKETS)?.candidates;
    let mut new_candidates = scan::scan_bytes(new.as_bytes(), BUCKETS)?.candidates;
    let found = anchor::find(&mut old_candidates, &mut new_candidates);
    let mut anchored = 0_u64;
    let mut start = Anchor { old: 0, new: 0 };
    for boundary in &found.anchors {
        anchored += diff_line_ranges(&old_lines, &new_lines, start, *boundary);
        start = *boundary;
    }
    let end = Anchor {
        old: line_number(old_lines.len() as u64),
        new: line_number(new_lines.len() as u64),
    };
    anchored += diff_line_ranges(&old_lines, &new_lines, start, end);
    let (whole, _) = diff_text(old, new);
    Ok((anchored, whole))
}

fn diff_line_ranges(old: &[&str], new: &[&str], start: Anchor, end: Anchor) -> u64 {
    let old = slice(old, start.old, end.old);
    let new = slice(new, start.new, end.new);
    if old.is_empty() && new.is_empty() {
        return 0;
    }
    let old = old.concat();
    let new = new.concat();
    diff_text(&old, &new).0
}

fn slice<'a>(lines: &'a [&'a str], start: u32, end: u32) -> &'a [&'a str] {
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let end = usize::try_from(end).unwrap_or(usize::MAX);
    lines.get(start..end).unwrap_or(&[])
}

fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split_inclusive('\n').collect()
    }
}

fn line_number(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
