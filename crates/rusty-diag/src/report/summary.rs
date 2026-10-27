use std::fmt::Write;

use super::ReportInput;
use crate::components::ComponentUse;
use crate::{CacheUsage, MemoryReport, Redactor};

pub(super) const SECTIONS: [&str; 7] = [
    "build",
    "system",
    "configuration",
    "errors",
    "recent_events",
    "memory",
    "performance",
];

const PRIVACY: &str = "This report holds no file names, user or computer names; paths are replaced by tokens like <d:3fa2>.";

pub(super) fn summary_text(input: &ReportInput, redactor: &Redactor) -> String {
    let memory = input.memory.as_ref().map_or_else(
        || "not available".to_owned(),
        |m| format!("{} caches, {} task owners", m.caches.len(), m.tasks.len()),
    );
    let mut text = String::from("Navo diagnostic report (report.json)\n\n");
    let caches = input
        .memory
        .iter()
        .flat_map(|m| &m.caches)
        .map(|cache| cache_line(cache, redactor))
        .chain(input.memory.iter().flat_map(|m| heap_lines(m, redactor)));
    let lines = [
        "Build: version and commit.".to_owned(),
        "System: OS, CPU count, RAM, GPU, display scale and DPI awareness.".to_owned(),
        format!(
            "Configuration: {} changed settings.",
            input.settings_changed.len()
        ),
        format!("Errors: {}.", input.errors.len()),
        format!("Recent events: {}.", input.events.len()),
        format!("Memory: {memory}."),
    ]
    .into_iter()
    .chain(caches)
    .chain([
        format!("Performance: {} start-up marks.", input.startup_ms.len()),
        format!("Redaction misses: {}.", input.redaction_misses),
    ]);
    for line in lines {
        let _ = writeln!(text, "{line}");
    }
    let _ = writeln!(text, "\n{PRIVACY}");
    text
}

fn cache_line(cache: &CacheUsage, redactor: &Redactor) -> String {
    let size = &cache.size;
    format!(
        "  {}: {} of {} budget ({} pinned, {} unpinned), {} entries.",
        redactor.text(&cache.name),
        amount(size.bytes),
        amount(cache.budget),
        amount(size.pinned_bytes),
        amount(size.unpinned_bytes()),
        size.entries,
    )
}

fn heap_lines(memory: &MemoryReport, redactor: &Redactor) -> Vec<String> {
    let private = memory
        .process
        .map_or_else(|| "not measured".to_owned(), |p| amount(p.private_bytes));
    let heap = memory.heap.map_or_else(
        || format!("  Rust heap: not counted (process private {private})."),
        |heap| {
            format!(
                "  Rust heap: {} live, {} peak (process private {private}).",
                amount(heap.live_bytes),
                amount(heap.peak_bytes)
            )
        },
    );
    let components = memory
        .components
        .iter()
        .map(|component| component_line(component, redactor));
    std::iter::once(heap).chain(components).collect()
}

fn component_line(component: &ComponentUse, redactor: &Redactor) -> String {
    let name = redactor.text(component.name);
    if component.busy {
        format!("  {name}: busy.")
    } else {
        format!(
            "  {name}: {} in {} items.",
            amount(component.bytes),
            component.items
        )
    }
}

pub(crate) fn amount(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes < KIB * KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{:.1} MiB", bytes / (KIB * KIB))
    }
}
