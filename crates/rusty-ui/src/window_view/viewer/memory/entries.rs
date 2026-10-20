use std::ops::Range;

use rusty_automotive::{Image, LayoutOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub span: Range<u64>,
    pub is_gap: bool,
    pub folded: u64,
}

pub(super) fn entries(image: &Image) -> Vec<Entry> {
    let fold_under = LayoutOptions::default().fold_under;
    let mut out: Vec<Entry> = Vec::with_capacity(image.ranges.len() * 2);
    let mut previous: Option<&Range<u64>> = None;
    for range in &image.ranges {
        let gap = previous.map(|before| before.end..range.start);
        match (gap, out.last_mut()) {
            (Some(gap), Some(last)) if gap.end - gap.start < fold_under => {
                last.span.end = range.end;
                last.folded += u64::from(gap.start < gap.end);
            }
            (gap, _) => {
                out.extend(gap.map(|span| Entry {
                    span,
                    is_gap: true,
                    folded: 0,
                }));
                out.push(Entry {
                    span: range.clone(),
                    is_gap: false,
                    folded: 0,
                });
            }
        }
        previous = Some(range);
    }
    out
}

pub(super) fn entry_at(entries: &[Entry], address: u64) -> Option<usize> {
    let at = entries.partition_point(|entry| entry.span.end <= address);
    entries
        .get(at)
        .filter(|entry| entry.span.start <= address)
        .map(|_| at)
}

pub(super) fn name(entries: &[Entry], at: usize) -> String {
    match entries.get(at) {
        Some(entry) if entry.is_gap => "Gap".to_owned(),
        Some(_) => format!("Range {}", at / 2 + 1),
        None => String::new(),
    }
}
