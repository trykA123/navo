use super::*;

fn count_files(path: &Path) -> (usize, usize) {
    let mut files = 0;
    let mut deepest = 0;
    let mut stack = vec![(path.to_path_buf(), 0)];
    while let Some((folder, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push((entry.path(), depth + 1));
            } else {
                files += 1;
            }
        }
    }
    (files, deepest)
}

#[test]
fn the_plan_is_the_same_for_the_same_size() {
    assert_eq!(Plan::new(20_000), Plan::new(20_000));
}

#[test]
fn the_plan_holds_exactly_the_requested_files_with_huge_folders() {
    let plan = Plan::new(40_000);
    assert_eq!(plan.files, 40_000);
    assert_eq!(plan.huge, [10_000, 2_000]);
    assert_eq!(plan.operations, 4_000);
    let big = Plan::new(4_000_000);
    assert_eq!(big.huge, [500_000, 100_000]);
    assert_eq!(big.operations, 10_000);
    assert!(big.folders.iter().all(|folder| folder.depth <= MAX_DEPTH));
}

#[test]
fn creation_writes_every_planned_file_up_to_the_deepest_level() {
    let root = tempfile::tempdir().unwrap();
    let plan = Plan::new(5_000);
    create(root.path(), &plan).unwrap();
    assert_eq!(count_files(root.path()), (5_000, MAX_DEPTH));
    let deep: PathBuf = ["deep"]
        .into_iter()
        .map(str::to_owned)
        .chain((2..=MAX_DEPTH).map(|depth| format!("level-{depth}")))
        .collect();
    assert!(root.path().join(&deep).join(UNICODE_FILE).is_file());
    assert!(root.path().join(INCREMENTAL).join(victim_name(0)).is_file());
}

#[test]
fn creation_never_overwrites_an_existing_file() {
    let root = tempfile::tempdir().unwrap();
    let plan = Plan::new(5_000);
    let folder = root.path().join(INCREMENTAL);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(victim_name(0)), b"keep").unwrap();
    let error = create(root.path(), &plan).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(folder.join(victim_name(0))).unwrap(), b"keep");
}
