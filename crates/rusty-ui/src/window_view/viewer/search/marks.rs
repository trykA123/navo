use std::ops::Range;

use rusty_core::marks::{MarkError, MarkSet};
use rusty_core::paging::Needle;

use super::Marked;
use super::count::{CountUpdate, needle_width, start_count};
use super::jobs::Job;
use crate::prelude::*;

pub(in crate::window_view::viewer) struct MarkHits {
    needle: Needle,
    width: u64,
    starts: Vec<u64>,
    _job: Option<Job>,
}

#[derive(Default)]
pub(in crate::window_view::viewer) struct MarkLayer {
    pub set: MarkSet,
    pub rev: u64,
    hits: Vec<MarkHits>,
}

impl MarkLayer {
    fn colour_of(&self, hits: &MarkHits) -> Option<u8> {
        self.set.colour_of(&hits.needle)
    }

    pub(in crate::window_view::viewer) fn ranges_in(
        &self,
        span: Range<u64>,
    ) -> Vec<(Range<u64>, u8)> {
        let mut ranges = Vec::new();
        for hits in &self.hits {
            let Some(colour) = self.colour_of(hits) else {
                continue;
            };
            let first = hits
                .starts
                .partition_point(|start| start + hits.width <= span.start);
            let inside = hits.starts[first..]
                .iter()
                .take_while(|start| **start < span.end);
            ranges.extend(inside.map(|start| (*start..start + hits.width, colour)));
        }
        ranges
    }

    pub(in crate::window_view::viewer) fn spans(&self) -> Vec<(u64, u8)> {
        self.hits
            .iter()
            .filter_map(|hits| Some((hits, self.colour_of(hits)?)))
            .flat_map(|(hits, colour)| hits.starts.iter().map(move |start| (*start, colour)))
            .collect()
    }

    fn landed(&mut self, needle: &Needle, update: CountUpdate) {
        let Some(hits) = self.hits.iter_mut().find(|hits| hits.needle == *needle) else {
            return;
        };
        match update {
            CountUpdate::Done(counted) => hits.starts = counted.starts,
            CountUpdate::Failed => hits.starts.clear(),
            CountUpdate::Running(_) => return,
        }
        hits._job = None;
        self.rev += 1;
    }
}

fn count_mark<V: Marked>(view: &mut V, needle: Needle, cx: &mut Context<V>) {
    let source = view.marking().source.clone();
    let key = needle.clone();
    let job = start_count(
        source,
        needle.clone(),
        cx,
        move |view: &mut V, update, cx| {
            let marking = view.marking();
            marking.marks.landed(&key, update);
            marking.rev += 1;
            cx.notify();
        },
    );
    let layer = &mut view.marking().marks;
    match layer.hits.iter_mut().find(|hits| hits.needle == needle) {
        Some(hits) => hits._job = Some(job),
        None => layer.hits.push(MarkHits {
            width: needle_width(&needle),
            needle,
            starts: Vec::new(),
            _job: Some(job),
        }),
    }
}

pub(in crate::window_view::viewer) fn recount_marks<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let needles: Vec<Needle> = view
        .marking()
        .marks
        .set
        .iter()
        .map(|mark| mark.needle.clone())
        .collect();
    let layer = &mut view.marking().marks;
    layer.hits.retain(|hits| needles.contains(&hits.needle));
    layer.rev += 1;
    for needle in needles {
        count_mark(view, needle, cx);
    }
}

fn mark_error(error: &MarkError) -> String {
    match error {
        MarkError::Full => "A file holds at most 32 marks. Ctrl+0 clears them.".to_owned(),
        other => other.to_string(),
    }
}

pub(in crate::window_view::viewer) fn toggle_mark<V: Marked>(
    view: &mut V,
    needle: Needle,
    colour: u8,
    cx: &mut Context<V>,
) -> Result<(), String> {
    let layer = &mut view.marking().marks;
    if layer.set.colour_of(&needle) == Some(colour) {
        layer.set.remove(&needle);
        layer.hits.retain(|hits| hits.needle != needle);
    } else {
        let is_new = layer.set.colour_of(&needle).is_none();
        layer
            .set
            .set(needle.clone(), colour)
            .map_err(|error| mark_error(&error))?;
        if is_new {
            count_mark(view, needle, cx);
        }
    }
    marks_changed(view, cx);
    Ok(())
}

pub(in crate::window_view::viewer) fn clear_marks<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let layer = &mut view.marking().marks;
    layer.set.clear();
    layer.hits.clear();
    marks_changed(view, cx);
}

pub(in crate::window_view::viewer) fn marks_changed<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let marking = view.marking();
    marking.rev += 1;
    marking.marks.rev += 1;
    super::memory::memory_changed(view, cx);
    cx.notify();
}
