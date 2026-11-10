//! Instruction-count benches for Ctrl+K search and index load under Callgrind.
//! No gungraun limits are set: `cargo xtask speed-verdict` judges the summaries.

#![allow(
    clippy::unwrap_used,
    reason = "a broken fixture must stop the run, not measure an error path"
)]

use std::hint::black_box;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use gungraun::{
    Callgrind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use navo_index::file::{self, Header};
use navo_index::search::{TableHit, Term, search};
use navo_index::table::{Flags, NO_PARENT, NameTable};
use tempfile::TempDir;

const ENTRIES: u32 = 250_000;
const FOLDERS: u32 = 500;
const FILES: u32 = ENTRIES - 1 - FOLDERS;
const LIMIT: usize = 200;

/// Builds the 250,000-entry table: one root, 500 folders, and cyclic file names.
fn build_table() -> NameTable {
    let mut table = NameTable::with_capacity(ENTRIES as usize, 6 * 1024 * 1024);
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut folders = Vec::with_capacity(FOLDERS as usize);
    for number in 0..FOLDERS {
        let name = if number % 100 == 7 {
            format!("huge-{number:03}")
        } else {
            format!("dir-{number:03}")
        };
        folders.push(table.push(root, &name, Flags::FOLDER).unwrap());
    }
    for file in 0..FILES {
        let name = match file % 4 {
            0 => format!("sensor-{file:06}.bin"),
            1 => format!("object-{file:06}.txt"),
            2 => format!("report {file:06}.pdf"),
            _ => format!("résumé-{file:06}.md"),
        };
        // Groups of four consecutive files share a folder, so every folder holds all four kinds.
        let folder = folders[((file / 4) % FOLDERS) as usize];
        table.push(folder, &name, Flags::NONE).unwrap();
    }
    table
}

/// Returns the shared table, built once outside every counted region.
fn table() -> Arc<NameTable> {
    static TABLE: OnceLock<Arc<NameTable>> = OnceLock::new();
    TABLE.get_or_init(|| Arc::new(build_table())).clone()
}

/// One search query, prebuilt with its table and extension strings.
struct SearchCase {
    table: Arc<NameTable>,
    query: Query,
    extensions: Vec<String>,
}

#[derive(Clone, Copy)]
enum Query {
    Name(&'static str),
    Path(&'static str),
    Ext,
}

/// Builds a search case and asserts that its query has at least one hit.
fn search_case(query: Query) -> SearchCase {
    let case = SearchCase {
        table: table(),
        query,
        extensions: vec!["bin".to_owned()],
    };
    assert!(!run_search(&case).is_empty(), "the query has no hits");
    case
}

/// Runs the case's query with a 200-hit limit and a cancel token that never fires.
fn run_search(case: &SearchCase) -> Vec<TableHit> {
    let terms = match case.query {
        Query::Name(text) => vec![Term::Name(text)],
        Query::Path(text) => vec![Term::Path(text)],
        Query::Ext => vec![Term::Ext(&case.extensions)],
    };
    search(&case.table, &terms, LIMIT, &|| false)
}

/// Search by name: the common `sensor` term.
fn name_case() -> SearchCase {
    search_case(Query::Name("sensor"))
}

/// Search by fuzzy name: `snsr`.
fn fuzzy_case() -> SearchCase {
    search_case(Query::Name("snsr"))
}

/// Search by path fragment: `huge-007/sensor`.
fn path_case() -> SearchCase {
    search_case(Query::Path("huge-007/sensor"))
}

/// Search by extension: `bin`.
fn extension_case() -> SearchCase {
    search_case(Query::Ext)
}

#[library_benchmark]
#[bench::sensor(name_case())]
fn bench_name(case: SearchCase) -> Vec<TableHit> {
    black_box(run_search(&case))
}

#[library_benchmark]
#[bench::snsr(fuzzy_case())]
fn bench_fuzzy(case: SearchCase) -> Vec<TableHit> {
    black_box(run_search(&case))
}

#[library_benchmark]
#[bench::huge_007_sensor(path_case())]
fn bench_path(case: SearchCase) -> Vec<TableHit> {
    black_box(run_search(&case))
}

#[library_benchmark]
#[bench::bin(extension_case())]
fn bench_extension(case: SearchCase) -> Vec<TableHit> {
    black_box(run_search(&case))
}

/// A saved index file and the temp folder that owns it.
struct LoadCase {
    _dir: TempDir,
    path: PathBuf,
}

/// Saves the table into a temporary index file and asserts that it loads again.
fn load_case() -> LoadCase {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.bin");
    let header = Header {
        volume_serial: 1,
        journal_id: 2,
        next_usn: 3,
        root: "C:".to_owned(),
        built_at: 0,
    };
    let table = table();
    file::save(&path, &header, &table).unwrap();
    assert!(file::load(&path).is_ok(), "the saved index does not load");
    LoadCase { _dir: dir, path }
}

#[library_benchmark]
#[bench::index(load_case())]
fn bench_load(case: LoadCase) -> usize {
    let (_, table) = file::load(&case.path).unwrap();
    black_box(table.len())
}

/// Callgrind with default options and no soft limits.
fn callgrind() -> Callgrind {
    Callgrind::default()
}

/// Bench config: Callgrind plus one rayon thread, so counts do not depend on scheduling.
fn config() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(callgrind()).env("RAYON_NUM_THREADS", "1");
    config
}

library_benchmark_group!(
    name = search,
    benchmarks = [
        bench_name,
        bench_fuzzy,
        bench_path,
        bench_extension,
        bench_load
    ]
);

main!(config = config(), library_benchmark_groups = search);
