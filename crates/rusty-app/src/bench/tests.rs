use std::fs;

use rusty_index::table::{Flags, NO_PARENT, NameTable};

use super::*;

fn options(root: &Path, out: &Path) -> Options {
    Options {
        root: root.to_path_buf(),
        files: 5_000,
        keep: false,
        out: Some(out.join("bench.json")),
    }
}

fn parsed(args: &[&str]) -> Result<Options, String> {
    parse(args.iter().map(OsString::from))
}

#[test]
fn the_whole_bench_reports_every_section_and_removes_its_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let (root, out) = (temp.path().join("work"), temp.path().join("out"));
    fs::create_dir(&out).unwrap();
    let (report, json, md) = execute(&options(&root, &out)).unwrap();

    assert_eq!(report.generation.files, 5_000);
    assert!(report.index.entries >= 5_000);
    assert!(report.index.snapshot_bytes > 0);
    assert_eq!(report.search.len(), 11);
    let found = |label: &str| {
        report
            .search
            .iter()
            .find(|row| row.label == label)
            .unwrap()
            .results
    };
    for row in &report.search {
        if ["no match", "fuzzy, swapped letters"].contains(&row.label.as_str()) {
            assert_eq!(row.results, 0, "{} matched", row.label);
        } else {
            assert!(row.results > 0, "{} found nothing", row.label);
        }
    }
    assert!(found("exact name") >= 1);
    assert!(report.search.iter().all(|row| row.warm_runs == 50));
    let entries: Vec<_> = report.listing.iter().map(|row| row.entries).collect();
    assert_eq!(entries, [1_250, 250]);
    assert!(report.listing.iter().all(|row| row.sort_ms.len() == 4));
    assert!(report.listing.iter().all(|row| row.filter_matches > 0));
    assert_eq!(report.incremental.measured, cfg!(windows));

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(json).unwrap()).unwrap();
    for section in ["generation", "index", "search", "listing", "incremental"] {
        assert!(written.get(section).is_some(), "missing {section}");
    }
    assert!(
        fs::read_to_string(md)
            .unwrap()
            .contains("## Incremental update")
    );
    assert!(
        !root.exists(),
        "the bench created the root and should remove it"
    );
}

#[test]
fn keep_leaves_the_fixture_and_removes_only_the_index() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("work");
    let mut kept = options(&root, temp.path());
    kept.keep = true;
    execute(&kept).unwrap();
    assert!(root.join("fixture").join(fixture::INCREMENTAL).is_dir());
    assert!(!root.join("index").exists());
}

#[test]
fn a_root_with_files_in_it_is_refused_and_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("work");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("notes.txt"), b"mine").unwrap();
    let error = execute(&options(&root, temp.path())).unwrap_err();
    assert!(error.contains("not empty"), "{error}");
    assert_eq!(fs::read(root.join("notes.txt")).unwrap(), b"mine");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

#[test]
fn an_empty_root_given_by_the_user_stays_after_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("work");
    fs::create_dir(&root).unwrap();
    let workspace = Workspace::prepare(&root).unwrap();
    fs::write(workspace.fixture.join("a.c"), b"").unwrap();
    workspace.remove(false).unwrap();
    assert!(root.is_dir());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn a_failed_measurement_still_removes_the_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("work");
    let mut broken = options(&root, temp.path());
    broken.out = Some(temp.path().join("missing").join("bench.json"));
    let error = execute(&broken).unwrap_err();
    assert!(error.contains("bench.json"), "{error}");
    assert!(!root.exists());
}

#[test]
fn bench_arguments_are_read_with_defaults() {
    let options = parsed(&["--root", "fixture"]).unwrap();
    assert_eq!(options.files, DEFAULT_FILES);
    assert!(!options.keep);
    let options = parsed(&[
        "--files",
        "3,000,000",
        "--root",
        "r",
        "--keep",
        "--out",
        "x.json",
    ])
    .unwrap();
    assert_eq!(options.files, 3_000_000);
    assert!(options.keep);
    assert_eq!(options.out, Some(PathBuf::from("x.json")));
}

#[test]
fn bad_bench_arguments_are_refused() {
    assert!(parsed(&[]).unwrap_err().contains("--root"));
    assert!(parsed(&["--root"]).unwrap_err().contains("needs a value"));
    assert!(parsed(&["--root", "r", "--files", "ten"]).is_err());
    assert!(parsed(&["--root", "r", "--files", "10"]).is_err());
    assert!(
        parsed(&["--root", "r", "--fast"])
            .unwrap_err()
            .contains("--fast")
    );
}

#[test]
fn incremental_changes_split_into_creates_renames_and_deletes() {
    let changes = Changes::of(10_000);
    assert_eq!(
        (changes.created, changes.renamed, changes.deleted),
        (4_000, 3_000, 3_000)
    );
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join(fixture::INCREMENTAL);
    fs::create_dir(&folder).unwrap();
    for index in 0..60 {
        fs::write(folder.join(fixture::victim_name(index)), b"").unwrap();
    }
    incremental::apply(temp.path(), Changes::of(100)).unwrap();
    let mut table = NameTable::new();
    let parent = table.push(NO_PARENT, "incremental", Flags::FOLDER).unwrap();
    for entry in fs::read_dir(&folder).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        table.push(parent, &name, Flags::NONE).unwrap();
    }
    let seen = incremental::seen(&table);
    assert_eq!((seen.created, seen.renamed, seen.victims), (40, 30, 0));
}

#[test]
fn a_query_run_while_the_index_table_is_write_locked_still_reports_its_results() {
    use std::sync::{Arc, RwLock, mpsc};
    use std::time::Duration;

    let mut table = NameTable::new();
    let top = table.push(NO_PARENT, "/work", Flags::FOLDER).unwrap();
    for name in ["brake_controller_main.c", "日本語_1.c"] {
        table.push(top, name, Flags::NONE).unwrap();
    }
    let table = Arc::new(RwLock::new(table));
    let root = rusty_core::Location::new("/work").unwrap();
    let first = |rows: Vec<report::Search>| rows[0].results;

    assert_eq!(first(search::run(&root, table.clone())), 1);

    let (locked, ready) = mpsc::channel();
    let writer = {
        let table = table.clone();
        std::thread::spawn(move || {
            let _guard = table.write().unwrap();
            locked.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(300));
        })
    };
    ready.recv().unwrap();
    let during = first(search::run(&root, table.clone()));
    writer.join().unwrap();
    assert_eq!(
        during, 1,
        "the first query found nothing while the table was locked"
    );
}
