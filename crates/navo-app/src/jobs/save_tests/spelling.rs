use navo_core::{FileStamp, Location, write_plan};

use super::super::overwrite::staged_for;

#[test]
fn a_held_original_spelled_in_another_case_still_finds_its_staged_file() {
    let target = Location::new(r"C:\dest\a.txt").unwrap();
    let staged = Location::new(r"C:\dest\.a.txt.part").unwrap();
    let seen = FileStamp {
        size: 3,
        modified_ms: Some(1_000),
        hash: [1; 32],
    };
    let plan = write_plan(&target, &staged, seen, 11);
    let on_disk = Location::new(r"C:\dest\A.txt").unwrap();
    assert_eq!(staged_for(&plan, &on_disk), Some(staged));
    let elsewhere = Location::new(r"C:\dest\b.txt").unwrap();
    assert_eq!(staged_for(&plan, &elsewhere), None);
}
