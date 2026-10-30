use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput};
use rusty_core::paging::{ByteSource, LineIndex, read_full};
use rusty_fs::PagedFile;

use super::anchor::{self, Anchor};
use super::scan::{self, BUCKETS};

pub const SECTION_CAP_BYTES: u64 = 64 * 1024 * 1024;
pub const SECTION_CAP_LINES: u64 = 1_000_000;
const MIN_SECTION_LINES: u64 = 2048;
const REANCHOR_BUCKETS: u32 = BUCKETS / 8;
const REANCHOR_CANDIDATES: usize = 4_000_000;

#[derive(Clone, Copy)]
pub struct Side<'a> {
    pub file: &'a PagedFile,
    pub index: &'a LineIndex,
    pub lines: u64,
}

pub struct Sides<'a> {
    pub old: Side<'a>,
    pub new: Side<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub old_start: u32,
    pub old_end: u32,
    pub new_start: u32,
    pub new_end: u32,
    pub old_offset: u64,
    pub old_bytes: u64,
    pub new_offset: u64,
    pub new_bytes: u64,
    pub coarse: bool,
}

impl Section {
    pub fn old_lines(&self) -> u64 {
        u64::from(self.old_end - self.old_start)
    }

    pub fn new_lines(&self) -> u64 {
        u64::from(self.new_end - self.new_start)
    }

    pub fn working_bytes(&self) -> u64 {
        self.old_bytes + self.new_bytes + (self.old_lines() + self.new_lines()) * 24
    }
}

pub struct Plan {
    pub sections: Vec<Section>,
    pub reanchored: u64,
    pub tracked_bytes: usize,
}

impl Plan {
    pub fn coarse(&self) -> u64 {
        self.sections
            .iter()
            .filter(|section| section.coarse)
            .count() as u64
    }

    pub fn coarse_at(&self, budget: u64) -> u64 {
        self.sections
            .iter()
            .filter(|section| section.coarse || section.working_bytes() > budget)
            .count() as u64
    }
}

pub struct DiffResult {
    pub changed_lines: u64,
    pub hunks: u64,
    pub coarse: u64,
    pub tracked_bytes: usize,
}

#[derive(Clone, Copy)]
struct Span {
    start: Anchor,
    end: Anchor,
}

pub fn plan(sides: &Sides, anchors: &[Anchor]) -> Result<Plan, String> {
    let boundaries = select_boundaries(anchors);
    let mut sections = Vec::new();
    let mut start = Anchor { old: 0, new: 0 };
    for boundary in &boundaries {
        add_range(
            &mut sections,
            sides,
            Span {
                start,
                end: *boundary,
            },
        )?;
        start = *boundary;
    }
    let end = Anchor {
        old: to_line(sides.old.lines),
        new: to_line(sides.new.lines),
    };
    add_range(&mut sections, sides, Span { start, end })?;
    let (sections, reanchored) = expand(sections, sides)?;
    let tracked = sections.len() * std::mem::size_of::<Section>()
        + boundaries.len() * std::mem::size_of::<Anchor>();
    Ok(Plan {
        sections,
        reanchored,
        tracked_bytes: tracked,
    })
}

fn to_line(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn expand(sections: Vec<Section>, sides: &Sides) -> Result<(Vec<Section>, u64), String> {
    let mut expanded = Vec::with_capacity(sections.len());
    let mut reanchored = 0;
    for section in sections {
        if !over_cap(&section) {
            expanded.push(section);
            continue;
        }
        reanchored += 1;
        match reanchor(&section, sides)? {
            Some(parts) => expanded.extend(parts),
            None => expanded.push(Section {
                coarse: true,
                ..section
            }),
        }
    }
    Ok((expanded, reanchored))
}

fn select_boundaries(anchors: &[Anchor]) -> Vec<Anchor> {
    let mut selected = Vec::new();
    let mut previous = Anchor { old: 0, new: 0 };
    let mut old_span = 0_u64;
    let mut new_span = 0_u64;
    for anchor in anchors {
        if anchor.old < previous.old || anchor.new < previous.new {
            continue;
        }
        old_span += u64::from(anchor.old - previous.old);
        new_span += u64::from(anchor.new - previous.new);
        previous = *anchor;
        if old_span + new_span >= MIN_SECTION_LINES {
            selected.push(*anchor);
            old_span = 0;
            new_span = 0;
        }
    }
    selected
}

fn add_range(sections: &mut Vec<Section>, sides: &Sides, span: Span) -> Result<(), String> {
    if span.start.old >= span.end.old && span.start.new >= span.end.new {
        return Ok(());
    }
    sections.push(make_section(sides, span)?);
    Ok(())
}

fn make_section(sides: &Sides, span: Span) -> Result<Section, String> {
    let old_start = line_offset(&sides.old, span.start.old)?;
    let old_end = line_offset(&sides.old, span.end.old)?;
    let new_start = line_offset(&sides.new, span.start.new)?;
    let new_end = line_offset(&sides.new, span.end.new)?;
    Ok(Section {
        old_start: span.start.old,
        old_end: span.end.old,
        new_start: span.start.new,
        new_end: span.end.new,
        old_offset: old_start,
        old_bytes: old_end.saturating_sub(old_start),
        new_offset: new_start,
        new_bytes: new_end.saturating_sub(new_start),
        coarse: false,
    })
}

fn line_offset(side: &Side, line: u32) -> Result<u64, String> {
    if line == 0 {
        return Ok(0);
    }
    match side.index.line_start(side.file, u64::from(line) + 1)? {
        Some(offset) => Ok(offset),
        None if u64::from(line) >= side.index.newlines() => Ok(side.file.len()),
        None => Err(format!("line {} is outside the scan", u64::from(line) + 1)),
    }
}

fn over_cap(section: &Section) -> bool {
    section.old_bytes > SECTION_CAP_BYTES
        || section.new_bytes > SECTION_CAP_BYTES
        || section.old_lines() > SECTION_CAP_LINES
        || section.new_lines() > SECTION_CAP_LINES
}

#[derive(Clone)]
struct Ranges {
    old: Range<u64>,
    new: Range<u64>,
}

fn reanchor(section: &Section, sides: &Sides) -> Result<Option<Vec<Section>>, String> {
    let old_start = line_offset(&sides.old, section.old_start)?;
    let old_end = line_offset(&sides.old, section.old_end)?;
    let new_start = line_offset(&sides.new, section.new_start)?;
    let new_end = line_offset(&sides.new, section.new_end)?;
    let ranges = Ranges {
        old: old_start..old_end,
        new: new_start..new_end,
    };
    for buckets in [REANCHOR_BUCKETS, 1] {
        if let Some(parts) = reanchor_with(section, sides, &ranges, buckets)? {
            return Ok(Some(parts));
        }
    }
    Ok(None)
}

fn reanchor_with(
    section: &Section,
    sides: &Sides,
    ranges: &Ranges,
    buckets: u32,
) -> Result<Option<Vec<Section>>, String> {
    let Some(mut old) = scan_side(&sides.old, ranges.old.clone(), section.old_start, buckets)?
    else {
        return Ok(None);
    };
    let Some(mut new) = scan_side(&sides.new, ranges.new.clone(), section.new_start, buckets)?
    else {
        return Ok(None);
    };
    let found = anchor::find(&mut old, &mut new);
    if found.anchors.is_empty() {
        return Ok(None);
    }
    let boundaries = select_boundaries(&found.anchors);
    let mut parts = Vec::new();
    let mut start = Anchor {
        old: section.old_start,
        new: section.new_start,
    };
    let end = Anchor {
        old: section.old_end,
        new: section.new_end,
    };
    for boundary in &boundaries {
        add_range(
            &mut parts,
            sides,
            Span {
                start,
                end: *boundary,
            },
        )?;
        start = *boundary;
    }
    add_range(&mut parts, sides, Span { start, end })?;
    if parts.iter().any(over_cap) {
        return Ok(None);
    }
    Ok(Some(parts))
}

fn scan_side(
    side: &Side,
    range: Range<u64>,
    first_line: u32,
    buckets: u32,
) -> Result<Option<Vec<scan::Candidate>>, String> {
    let options = scan::ScanOptions {
        first_line,
        buckets,
        candidate_cap: REANCHOR_CANDIDATES,
    };
    match scan::scan_range(side.file, range.start, range.end, options) {
        Ok(scanned) => Ok(Some(scanned.candidates)),
        Err(_) => Ok(None),
    }
}

pub fn diff(
    old_file: &PagedFile,
    new_file: &PagedFile,
    sections: &[Section],
    budget: u64,
) -> Result<DiffResult, String> {
    let mut result = DiffResult {
        changed_lines: 0,
        hunks: 0,
        coarse: 0,
        tracked_bytes: 0,
    };
    for section in sections {
        if section.coarse || section.working_bytes() > budget {
            result.changed_lines += section.old_lines() + section.new_lines();
            result.hunks += 1;
            result.coarse += 1;
            continue;
        }
        let old_text = read_text_bytes(
            old_file,
            section.old_offset,
            section.old_offset + section.old_bytes,
        )?;
        let new_text = read_text_bytes(
            new_file,
            section.new_offset,
            section.new_offset + section.new_bytes,
        )?;
        let (Some(old_text), Some(new_text)) = (old_text, new_text) else {
            result.changed_lines += section.old_lines() + section.new_lines();
            result.hunks += 1;
            result.coarse += 1;
            continue;
        };
        let used = old_text.len() + new_text.len();
        let tracked = used + (section.old_lines() + section.new_lines()) as usize * 24;
        result.tracked_bytes = result.tracked_bytes.max(tracked);
        let (changed, hunks) = diff_text(&old_text, &new_text);
        result.changed_lines += changed;
        result.hunks += hunks;
    }
    Ok(result)
}

pub fn provisional(old_file: &PagedFile, new_file: &PagedFile, cap: u64) -> Result<(), String> {
    let old = read_text_bytes(old_file, 0, cap.min(old_file.len()))?
        .ok_or("provisional old is not text")?;
    let new = read_text_bytes(new_file, 0, cap.min(new_file.len()))?
        .ok_or("provisional new is not text")?;
    std::hint::black_box(diff_text(&old, &new));
    Ok(())
}

pub fn read_text_bytes(file: &PagedFile, start: u64, end: u64) -> Result<Option<String>, String> {
    let len = usize::try_from(end.saturating_sub(start)).map_err(|_| "range is too large")?;
    let mut bytes = vec![0_u8; len];
    let read = read_full(file, start, &mut bytes)?;
    if read != len {
        return Err(format!("short read at {start}: {read} of {len}"));
    }
    Ok(String::from_utf8(bytes).ok())
}

pub fn diff_text(old: &str, new: &str) -> (u64, u64) {
    let input = InternedInput::new(old, new);
    let diff = Diff::compute(Algorithm::Histogram, &input);
    let mut changed = 0_u64;
    let mut hunks = 0_u64;
    for hunk in diff.hunks() {
        changed += u64::from(hunk.before.end - hunk.before.start);
        changed += u64::from(hunk.after.end - hunk.after.start);
        hunks += 1;
    }
    (changed, hunks)
}
