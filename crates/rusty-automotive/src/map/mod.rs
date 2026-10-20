mod finish;
mod lines;
mod model;
mod parts;
mod query;
mod retention;
mod rows;
mod scan;
mod table;
mod tasking;
mod wrap;

pub use model::{
    Contribution, DiscardedSection, MapError, MapFlavor, MapModel, MapProblem, MemoryRegion,
    OutputSection, Symbol,
};
pub use query::{ObjectTotal, RegionTotal, SymbolIndex};
pub use scan::index_map;

pub const MAX_MAP_REGIONS: usize = 16_384;
pub const MAX_MAP_SECTIONS: usize = 65_536;
pub const MAX_MAP_CONTRIBUTIONS: usize = 250_000;
pub const MAX_MAP_DISCARDED: usize = 100_000;
pub const MAX_MAP_ENTRIES: usize = 500_000;
pub const MAX_MAP_TEXT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_MAP_PROBLEMS: usize = 1000;
pub const MAX_MAP_LINE: usize = 4096;

#[must_use]
pub fn sniff_map(head: &[u8]) -> Option<MapFlavor> {
    let mut evidence = parts::Evidence::default();
    head.split(|byte| *byte == b'\n')
        .take(200)
        .any(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.len() > MAX_MAP_LINE {
                evidence.take("")
            } else {
                evidence.take(&String::from_utf8_lossy(line))
            }
        })
        .then_some(MapFlavor::Tasking)
}
