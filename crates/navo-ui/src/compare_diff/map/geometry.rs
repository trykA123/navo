//! Pure geometry of the change map: tick positions, merging, viewport box, click to row.
//! The map element calls these functions; the tests call them directly.

use std::ops::Range;

/// The smallest drawn height of a tick or of the viewport box, in px.
pub const MIN_HEIGHT: f32 = 3.;
/// Ticks closer than this many px merge into one.
const MERGE_GAP: f32 = 1.;

/// What a hunk does; it picks the tick colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    Changed,
    Added,
    Deleted,
}

/// One hunk as the map sees it: a row range and a kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapHunk {
    pub rows: Range<u32>,
    pub kind: HunkKind,
}

/// One drawn tick. It stands for the hunks `hunks` (more than one when merged).
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub top: f32,
    pub height: f32,
    pub kind: HunkKind,
    pub hunks: Range<usize>,
}

/// Maps a row to its position in px along a strip of `height` px.
fn at(row: u32, total: u32, height: f32) -> f32 {
    row as f32 / total.max(1) as f32 * height
}

/// Places one tick per hunk along `height` px, merging ticks less than 1 px apart.
/// A merged tick of mixed kinds is `Changed`. Every tick is at least `MIN_HEIGHT` tall
/// and stays inside the strip. Hunks must be sorted by row.
pub fn ticks(hunks: &[MapHunk], total_rows: u32, height: f32) -> Vec<Tick> {
    // Each entry keeps its raw (unpadded) bottom so the next hunk can be compared to it.
    let mut out: Vec<(Tick, f32)> = Vec::new();
    for (index, hunk) in hunks.iter().enumerate() {
        let top = at(hunk.rows.start, total_rows, height);
        let bottom = at(hunk.rows.end, total_rows, height).max(top);
        if let Some((last, last_bottom)) = out.last_mut()
            && top - *last_bottom < MERGE_GAP
        {
            *last_bottom = last_bottom.max(bottom);
            last.height = *last_bottom - last.top;
            last.hunks.end = index + 1;
            if last.kind != hunk.kind {
                last.kind = HunkKind::Changed;
            }
            continue;
        }
        let tick = Tick {
            top,
            height: bottom - top,
            kind: hunk.kind,
            hunks: index..index + 1,
        };
        out.push((tick, bottom));
    }
    out.into_iter()
        .map(|(mut tick, _)| {
            tick.height = tick.height.max(MIN_HEIGHT).min(height.max(MIN_HEIGHT));
            tick.top = tick.top.min((height - tick.height).max(0.));
            tick
        })
        .collect()
}

/// Returns the viewport box `(top, height)` in px for the visible `rows`.
pub fn viewport_box(rows: &Range<u32>, total_rows: u32, height: f32) -> (f32, f32) {
    let top = at(rows.start.min(total_rows), total_rows, height);
    let bottom = at(rows.end.min(total_rows), total_rows, height);
    let box_height = (bottom - top).max(MIN_HEIGHT).min(height.max(MIN_HEIGHT));
    (top.min((height - box_height).max(0.)), box_height)
}

/// Returns the row under a pointer `y` px from the strip top; clamps to the first and last row.
pub fn row_at(y: f32, total_rows: u32, height: f32) -> u32 {
    if total_rows == 0 || height <= 0. {
        return 0;
    }
    let row = (y / height * total_rows as f32).floor();
    (row.max(0.) as u32).min(total_rows - 1)
}

/// Returns the index in `ticks` of the tick that holds `hunk`.
pub fn tick_of(ticks: &[Tick], hunk: usize) -> Option<usize> {
    ticks.iter().position(|tick| tick.hunks.contains(&hunk))
}
