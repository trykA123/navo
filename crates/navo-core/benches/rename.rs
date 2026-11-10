//! Instruction-count benches for rename previews and collision checks under Callgrind.
//! No gungraun limits are set: `cargo xtask speed-verdict` judges the summaries.

#![allow(
    clippy::unwrap_used,
    reason = "a broken fixture must stop the run, not measure an error path"
)]

use std::hint::black_box;

use gungraun::{
    Callgrind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use navo_core::Location;
use navo_core::rename::check::{Existing, ExistingName, Fold, Problem, Row, RowState, check_rows};
use navo_core::rename::{Compiled, Find, RenameRules, Subject, split_name};

const NAMES: usize = 10_000;
const COLLIDING: usize = 100;

/// The compiled rules and the 10,000 split names for the preview bench.
struct PreviewCase {
    compiled: Compiled,
    names: Vec<(String, String)>,
}

/// The rename rows and the existing names for the collision bench.
struct CheckCase {
    rows: Vec<Row>,
    existing: Existing,
}

/// Builds `IMG_NNNNN.JPG` and `scan NNNNN.pdf` names and the preview rules.
fn preview_case() -> PreviewCase {
    let rules = RenameRules {
        name_mask: "[N]_[C]".to_owned(),
        find: Find {
            text: "IMG".to_owned(),
            replace: "photo".to_owned(),
            ..Find::default()
        },
        ..RenameRules::default()
    };
    let compiled = Compiled::new(&rules).unwrap();
    let names = (0..NAMES)
        .map(|i| {
            let name = if i % 2 == 0 {
                format!("IMG_{i:05}.JPG")
            } else {
                format!("scan {i:05}.pdf")
            };
            let (stem, ext) = split_name(&name);
            (stem.to_owned(), ext.to_owned())
        })
        .collect();
    PreviewCase { compiled, names }
}

/// Builds the preview rows and asserts that only the 100 listed names collide.
fn check_case() -> CheckCase {
    let preview = preview_case();
    let parent = Location::new("/photos").unwrap();
    let rows: Vec<Row> = preview
        .names
        .iter()
        .enumerate()
        .map(|(index, (stem, ext))| {
            let subject = Subject {
                stem,
                ext,
                parent: "Photos",
                grand: "Trips",
                time: None,
                fields: None,
            };
            Row {
                parent: parent.clone(),
                old: format!("{stem}.{ext}"),
                new: preview.compiled.new_name(&subject, index as u64),
                excluded: false,
            }
        })
        .collect();
    let mut existing_names: Vec<ExistingName> = rows
        .iter()
        .map(|row| ExistingName {
            parent: parent.clone(),
            name: row.old.clone(),
        })
        .collect();
    existing_names.extend(rows.iter().take(COLLIDING).map(|row| ExistingName {
        parent: parent.clone(),
        name: row.new.clone(),
    }));
    let existing = Existing::new(existing_names);
    let states = check_rows(&rows, &existing, Fold::Exact);
    for (index, state) in states.iter().enumerate() {
        let expected = index < COLLIDING;
        assert_eq!(
            matches!(state, RowState::Problem(Problem::Exists)),
            expected,
            "row {index} has the wrong collision state"
        );
    }
    CheckCase { rows, existing }
}

#[library_benchmark]
#[bench::ten_thousand(preview_case())]
fn bench_preview(case: PreviewCase) -> Vec<String> {
    let mut names = Vec::with_capacity(case.names.len());
    for (index, (stem, ext)) in case.names.iter().enumerate() {
        let subject = Subject {
            stem,
            ext,
            parent: "Photos",
            grand: "Trips",
            time: None,
            fields: None,
        };
        names.push(case.compiled.new_name(&subject, index as u64));
    }
    black_box(names)
}

#[library_benchmark]
#[bench::ten_thousand(check_case())]
fn bench_check(case: CheckCase) -> Vec<RowState> {
    black_box(check_rows(&case.rows, &case.existing, Fold::Exact))
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

library_benchmark_group!(name = rename, benchmarks = [bench_preview, bench_check]);

main!(config = config(), library_benchmark_groups = rename);
