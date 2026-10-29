use std::hint::black_box;

use gungraun::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use rusty_core::{FsError, Location, ProviderPath};
use rusty_fs::view::{Needle, SortBy, SortKey, ViewFilter, filter_order, sort_order};
use rusty_fs::{Attributes, CancelToken, EntryKind, FakeFs, FolderSnapshot, FsEntry, load_folder};

const ROWS: u32 = 100_000;
const FAKE_ENTRIES: u32 = 50_000;

fn row(at: u32) -> FsEntry {
    let kind = if at.is_multiple_of(7) {
        EntryKind::Folder
    } else {
        EntryKind::File
    };
    let name = if at.is_multiple_of(3) {
        format!("sensor-{at:06}.bin")
    } else {
        format!("object-{at:06}.txt")
    };
    FsEntry {
        name: ProviderPath::from_text(&name),
        kind,
        size: u64::from(at) * 4_099 % 1_000_000,
        modified_ms: Some(i64::from(at) * 7_919 % 1_000_000_000),
        attributes: Attributes {
            hidden: at.is_multiple_of(11),
            system: at.is_multiple_of(13),
            readonly: at.is_multiple_of(17),
            link_to_folder: false,
        },
    }
}

fn generated_rows() -> Vec<FsEntry> {
    (0..ROWS).map(row).collect()
}

fn identity_order() -> Vec<u32> {
    (0..ROWS).collect()
}

fn sort_case(by: SortBy) -> (Vec<FsEntry>, SortKey, CancelToken) {
    (
        generated_rows(),
        SortKey {
            by,
            descending: false,
        },
        CancelToken::new(),
    )
}

fn filter_case() -> (Vec<FsEntry>, Vec<u32>, Needle, CancelToken) {
    (
        generated_rows(),
        identity_order(),
        Needle::parse("sensor"),
        CancelToken::new(),
    )
}

fn fake_folder() -> (FakeFs, Location) {
    let fs = FakeFs::new();
    fs.add_folder("d");
    for at in 0..FAKE_ENTRIES {
        fs.add_entry("d", row(at));
    }
    let location = fs.location("d");
    (fs, location)
}

#[library_benchmark]
#[bench::name(sort_case(SortBy::Name))]
#[bench::extension(sort_case(SortBy::Ext))]
#[bench::size(sort_case(SortBy::Size))]
#[bench::date(sort_case(SortBy::Date))]
fn bench_sort_order(input: (Vec<FsEntry>, SortKey, CancelToken)) -> Option<Vec<u32>> {
    let (rows, key, cancel) = input;
    black_box(sort_order(black_box(&rows), black_box(key), true, &cancel))
}

#[library_benchmark]
#[bench::sensor(filter_case())]
fn bench_filter_order(input: (Vec<FsEntry>, Vec<u32>, Needle, CancelToken)) -> Option<Vec<u32>> {
    let (rows, order, needle, cancel) = input;
    let filter = ViewFilter {
        needle: Some(&needle),
        show_hidden: true,
        show_system: true,
    };
    black_box(filter_order(
        black_box(&rows),
        black_box(&order),
        &filter,
        &cancel,
    ))
}

#[library_benchmark]
#[bench::fifty_thousand(fake_folder())]
fn bench_load_folder(input: (FakeFs, Location)) -> Result<FolderSnapshot, FsError> {
    let (fs, location) = input;
    black_box(load_folder(&fs, &location, &CancelToken::new()))
}

fn callgrind() -> Callgrind {
    let mut tool = Callgrind::default();
    tool.soft_limits([(EventKind::Ir, 10.0)]);
    tool
}

fn config() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(callgrind());
    config
}

library_benchmark_group!(
    name = listing,
    benchmarks = [bench_sort_order, bench_filter_order, bench_load_folder]
);

main!(config = config(), library_benchmark_groups = listing);
