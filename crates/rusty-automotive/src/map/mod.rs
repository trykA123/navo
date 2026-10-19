mod finish;
mod lines;
mod model;
mod rows;
mod scan;
mod table;
mod tasking;

pub use model::{
    Contribution, DiscardedSection, MapError, MapFlavor, MapModel, MapProblem, MemoryRegion,
    OutputSection, Symbol,
};
pub use scan::index_map;

pub const MAX_MAP_ENTRIES: usize = 500_000;
pub const MAX_MAP_TEXT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_MAP_PROBLEMS: usize = 1000;
pub const MAX_MAP_LINE: usize = 4096;

#[must_use]
pub fn sniff_map(head: &[u8]) -> Option<MapFlavor> {
    let head = head.strip_prefix(b"\xef\xbb\xbf").unwrap_or(head);
    head.split(|byte| *byte == b'\n').find_map(|line| {
        let text = std::str::from_utf8(line).ok()?.trim();
        let lower = text.to_ascii_lowercase();
        let banner = (lower.contains("tasking") || lower.contains("vx-toolset"))
            && (lower.contains("linker") || lower.contains("map"));
        let header = lower.strip_prefix('*').map(str::trim).is_some_and(|name| {
            name.starts_with("tool and invocation")
                || name.starts_with("locate result")
                || name.starts_with("symbols (sorted on ")
        });
        (banner || header).then_some(MapFlavor::Tasking)
    })
}
