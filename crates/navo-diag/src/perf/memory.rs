use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::time::Instant;

use serde::Serialize;

use super::{SECONDS, Summary, ms};
use crate::Redactor;
use crate::components::ComponentUse;
use crate::heap::HeapStats;
use crate::report::amount;

#[derive(Debug, Clone)]
struct Sampled {
    at: Instant,
    heap: Option<HeapStats>,
    components: Vec<ComponentUse>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryTrack {
    samples: VecDeque<Sampled>,
    dropped: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MemoryRecord {
    pub summary: MemorySummary,
    pub samples: Vec<MemoryRow>,
    pub dropped: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MemorySummary {
    pub heap_live_bytes: Option<Summary>,
    pub heap_peak_bytes: Option<u64>,
    pub heap_allocations: Option<u64>,
    pub components: Vec<ComponentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentSummary {
    pub name: String,
    pub last_bytes: Option<u64>,
    pub last_items: Option<u64>,
    pub max_bytes: u64,
    pub busy_samples: usize,
    pub busy_at_end: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemoryRow {
    pub at_ms: f64,
    pub heap_live_bytes: Option<u64>,
    pub heap_peak_bytes: Option<u64>,
    pub components: BTreeMap<String, Option<u64>>,
}

impl MemoryTrack {
    pub fn record(&mut self, at: Instant, heap: Option<HeapStats>, components: Vec<ComponentUse>) {
        if self.samples.len() >= SECONDS {
            self.samples.pop_front();
            self.dropped += 1;
        }
        self.samples.push_back(Sampled {
            at,
            heap,
            components,
        });
    }

    #[must_use]
    pub fn record_since(&self, started: Instant, redactor: &Redactor) -> MemoryRecord {
        let samples = self
            .samples
            .iter()
            .map(|sample| row(sample, started, redactor))
            .collect();
        let heaps = || self.samples.iter().filter_map(|sample| sample.heap);
        let last_heap = self.samples.iter().rev().find_map(|sample| sample.heap);
        MemoryRecord {
            summary: MemorySummary {
                heap_live_bytes: Summary::of(heaps().map(|heap| heap.live_bytes as f64)),
                heap_peak_bytes: last_heap.map(|heap| heap.peak_bytes),
                heap_allocations: last_heap.map(|heap| heap.allocations),
                components: self.components(redactor),
            },
            samples,
            dropped: self.dropped,
        }
    }

    fn components(&self, redactor: &Redactor) -> Vec<ComponentSummary> {
        let mut by_name: BTreeMap<&'static str, ComponentSummary> = BTreeMap::new();
        let last = self.samples.len().saturating_sub(1);
        for (index, sample) in self.samples.iter().enumerate() {
            for component in &sample.components {
                by_name
                    .entry(component.name)
                    .or_insert_with(|| ComponentSummary::named(redactor.text(component.name)))
                    .add(component, index == last);
            }
        }
        let mut components: Vec<ComponentSummary> = by_name.into_values().collect();
        components.sort_by(|a, b| {
            (b.last_bytes.unwrap_or(0), b.max_bytes)
                .cmp(&(a.last_bytes.unwrap_or(0), a.max_bytes))
                .then_with(|| a.name.cmp(&b.name))
        });
        components
    }
}

impl ComponentSummary {
    fn named(name: String) -> Self {
        Self {
            name,
            last_bytes: None,
            last_items: None,
            max_bytes: 0,
            busy_samples: 0,
            busy_at_end: false,
        }
    }

    fn add(&mut self, component: &ComponentUse, is_last: bool) {
        self.busy_samples += usize::from(component.busy);
        self.max_bytes = self.max_bytes.max(component.bytes);
        if is_last {
            self.busy_at_end = component.busy;
            self.last_bytes = (!component.busy).then_some(component.bytes);
            self.last_items = (!component.busy).then_some(component.items);
        }
    }
}

fn row(sample: &Sampled, started: Instant, redactor: &Redactor) -> MemoryRow {
    let at = ms(sample.at.saturating_duration_since(started));
    MemoryRow {
        at_ms: (f64::from(at) * 1000.0).round() / 1000.0,
        heap_live_bytes: sample.heap.map(|heap| heap.live_bytes),
        heap_peak_bytes: sample.heap.map(|heap| heap.peak_bytes),
        components: sample
            .components
            .iter()
            .map(|c| (redactor.text(c.name), (!c.busy).then_some(c.bytes)))
            .collect(),
    }
}

#[must_use]
pub fn memory_text(record: &MemoryRecord, private_bytes: Option<u64>) -> String {
    let summary = &record.summary;
    let live = record.samples.last().and_then(|row| row.heap_live_bytes);
    let mut text = String::from("Memory at the end of the recording\n");
    let private = private_bytes.map_or_else(|| "not measured".to_owned(), amount);
    let _ = writeln!(text, "Process private bytes: {private}");
    match (live, summary.heap_peak_bytes) {
        (Some(live), Some(peak)) => {
            let allocations = summary.heap_allocations.unwrap_or(0);
            let _ = writeln!(
                text,
                "Rust heap: {} live, {} peak, {allocations} allocations",
                amount(live),
                amount(peak)
            );
        }
        _ => text.push_str("Rust heap: not counted in this build\n"),
    }
    if let (Some(private), Some(live)) = (private_bytes, live) {
        let _ = writeln!(
            text,
            "Outside the Rust heap: {} (GPU driver, system libraries, fonts, thread stacks)",
            amount(private.saturating_sub(live))
        );
    }
    text.push_str("\nComponents, largest first\n");
    for component in &summary.components {
        let _ = writeln!(text, "{}", component_line(component));
    }
    if summary.components.is_empty() {
        text.push_str("No component reported its memory.\n");
    }
    text
}

fn component_line(component: &ComponentSummary) -> String {
    let max = amount(component.max_bytes);
    match (component.last_bytes, component.last_items) {
        (Some(bytes), Some(items)) => format!(
            "{}: {} in {items} items (max {max})",
            component.name,
            amount(bytes)
        ),
        _ if component.busy_at_end => {
            format!("{}: busy at the last sample (max {max})", component.name)
        }
        _ => format!("{}: gone at the end (max {max})", component.name),
    }
}
