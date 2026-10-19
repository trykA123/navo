use std::{fs, path::PathBuf};

use rusty_fs::DriveClass;

use super::{
    fixture::{self, DATA_SETS},
    options::{self, Options},
    report::{Measurement, markdown, median},
    workspace::{Workspace, simplified},
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn defaults_to_three_runs_and_all_three_sets() {
    assert_eq!(
        options::parse(&args(&["destination"])).unwrap(),
        Options {
            target: PathBuf::from("destination"),
            runs: 3,
            sets: DATA_SETS.to_vec(),
        }
    );
}

#[test]
fn parses_runs_and_selected_sets_in_either_option_order() {
    for values in [
        vec!["destination", "--runs", "2", "--sets", "medium,small"],
        vec!["destination", "--sets", "medium,small", "--runs", "2"],
    ] {
        let options = options::parse(&args(&values)).unwrap();
        assert_eq!(options.runs, 2);
        assert_eq!(options.sets, vec![DATA_SETS[1], DATA_SETS[0]]);
    }
}

#[test]
fn refuses_missing_invalid_and_repeated_arguments() {
    for values in [
        vec![],
        vec![""],
        vec!["--runs", "1"],
        vec!["destination", "--runs"],
        vec!["destination", "--runs", "0"],
        vec!["destination", "--runs", "-1"],
        vec!["destination", "--runs", "x"],
        vec!["destination", "--runs", "184467440737095516160"],
        vec!["destination", "--runs", "1", "--runs", "2"],
        vec!["destination", "--sets", ""],
        vec!["destination", "--sets", "small,"],
        vec!["destination", "--sets", "unknown"],
        vec!["destination", "--sets", "small,small"],
        vec!["destination", "--sets", "small", "--sets", "medium"],
        vec!["destination", "--unknown", "1"],
        vec!["destination", "extra"],
    ] {
        assert!(options::parse(&args(&values)).is_err(), "{values:?}");
    }
}

#[test]
fn data_sets_have_the_decided_file_counts_and_binary_sizes() {
    let actual = DATA_SETS.map(|set| (set.name, set.files, set.file_bytes, set.bytes()));
    assert_eq!(
        actual,
        [
            ("small", 10_000, 4_096, 40_960_000),
            ("medium", 200, 5_242_880, 1_048_576_000),
            ("large", 2, 1_073_741_824, 2_147_483_648),
        ]
    );
}

#[test]
fn fixture_content_is_repeatable_nonzero_and_changes_between_blocks() {
    let mut first = [0; 4096];
    let mut repeated = [0; 4096];
    let mut next = [0; 4096];
    let mut state = 42;
    fixture::fill(&mut state, &mut first);
    fixture::fill(&mut 42, &mut repeated);
    fixture::fill(&mut state, &mut next);
    assert_eq!(first, repeated);
    assert_ne!(first, next);
    assert!(first.iter().any(|byte| *byte != 0));
    assert!(first.windows(8).any(|pair| pair != &first[..8]));
}

#[test]
fn median_sorts_samples_and_averages_the_middle_pair() {
    assert_eq!(median(&mut [9.0, 1.0, 3.0]), Some(3.0));
    assert_eq!(median(&mut [9.0, 1.0, 5.0, 3.0]), Some(4.0));
    assert_eq!(median(&mut [2.0]), Some(2.0));
    assert_eq!(median(&mut []), None);
}

#[test]
fn markdown_reports_drive_classes_medians_and_decimal_megabytes_per_second() {
    let rows = [
        Measurement {
            set: DATA_SETS[0],
            method: "Standard",
            seconds: 2.0,
        },
        Measurement {
            set: DATA_SETS[0],
            method: "Auto",
            seconds: 1.0,
        },
    ];
    assert_eq!(
        markdown(&rows, [DriveClass::Ssd, DriveClass::Network], 8, 3),
        "Drive classes: source Ssd; target Network. Auto workers: 8.\n\n\
         3 run(s) per method; median elapsed time. MB = 1,000,000 bytes.\n\n\
         The destination is removed between runs; OS caches are not flushed.\n\n\
         | Set | Method | Median seconds | MB/s |\n\
         | --- | --- | ---: | ---: |\n\
         | small | Standard | 2.000 | 20.48 |\n\
         | small | Auto | 1.000 | 40.96 |\n"
    );
}

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ferrite-bench-test-{}-{timestamp}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_nonempty_target_is_refused_before_creating_or_removing_anything() {
    let root = TestRoot::new();
    let target = root.0.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"original").unwrap();
    fs::write(root.0.join("neighbor"), b"neighbor").unwrap();
    let options = Options {
        target: target.clone(),
        runs: 1,
        sets: vec![DATA_SETS[0]],
    };
    let error = super::benchmark(&options).unwrap_err();
    assert!(error.contains("target folder is not empty"));
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"original");
    assert_eq!(fs::read(root.0.join("neighbor")).unwrap(), b"neighbor");
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 2);
    assert_eq!(fs::read_dir(target).unwrap().count(), 1);
}

#[test]
fn cleanup_removes_created_folders_and_restores_an_existing_empty_target() {
    for existed in [false, true] {
        let root = TestRoot::new();
        let target = root.0.join("target");
        if existed {
            fs::create_dir(&target).unwrap();
        }
        let mut workspace = Workspace::new(&target).unwrap();
        workspace.prepare().unwrap();
        assert_eq!(workspace.source.parent(), workspace.target.parent());
        fs::write(workspace.source.join("fixture"), b"data").unwrap();
        fs::write(workspace.target.join("copy"), b"data").unwrap();
        workspace.reset_target().unwrap();
        assert_eq!(fs::read_dir(&workspace.target).unwrap().count(), 0);
        workspace.clean_up().unwrap();
        assert!(!workspace.source.exists());
        assert_eq!(target.exists(), existed);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), usize::from(existed));
    }
}

#[test]
fn setup_refuses_a_target_populated_after_validation_and_keeps_its_contents() {
    let root = TestRoot::new();
    let target = root.0.join("target");
    fs::create_dir(&target).unwrap();
    let mut workspace = Workspace::new(&target).unwrap();
    fs::write(target.join("keep"), b"original").unwrap();
    assert!(workspace.prepare().is_err());
    workspace.clean_up().unwrap();
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"original");
    assert!(!workspace.source.exists());
}

#[cfg(unix)]
#[test]
fn a_linked_target_is_refused_even_when_its_destination_is_empty() {
    let root = TestRoot::new();
    let real = root.0.join("real");
    let target = root.0.join("target");
    fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, &target).unwrap();
    assert!(Workspace::new(&target).is_err());
    assert!(target.is_symlink());
    assert!(real.is_dir());
    assert_eq!(fs::read_dir(root.0.join("real")).unwrap().count(), 0);
}

#[test]
fn simplified_strips_the_verbatim_prefix() {
    for (input, expected) in [
        (r"\\?\D:\a\_temp\small", r"D:\a\_temp\small"),
        (r"\\?\UNC\server\share\dir", r"\\server\share\dir"),
        (r"D:\plain\path", r"D:\plain\path"),
        ("/home/user/work", "/home/user/work"),
    ] {
        assert_eq!(simplified(&PathBuf::from(input)), PathBuf::from(expected));
    }
}
