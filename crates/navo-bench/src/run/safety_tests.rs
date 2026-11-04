use super::{Fakes, bench, planted};
use crate::fixture;
use crate::guard::test_support::{mkdir, write};
use crate::privacy::assert_no_leaks;
use crate::run::Exit;

#[test]
fn small_exit_codes_match_the_readme() {
    let codes = [Exit::Done, Exit::Failed, Exit::Usage, Exit::DataSafety].map(Exit::code);
    assert_eq!(codes, [0, 1, 2, 3]);
}

#[test]
fn medium_a_changed_owner_folder_exits_3_and_says_so() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(true);
    fakes.write_into = Some(fx.marker_file.clone());
    let ran = bench(&mut fakes, Some(fx.root()), &["--quick"]);
    assert_eq!(ran.exit, Exit::DataSafety);
    assert_eq!(ran.exit.code(), 3);
    let proof = ran.run_summary()["proof"].clone();
    assert_eq!(proof["state"], "fail");
    assert_eq!(proof["diff"]["verdict"], "changed");
    assert!(ran.file("report.html").contains("DATA SAFETY"));
    assert!(String::from_utf8_lossy(&ran.console).contains("DATA SAFETY: FAIL"));
    assert_eq!(fakes.run_roots_left(), 0);
}

#[test]
fn medium_a_protected_name_that_matches_the_safety_words_cannot_hide_a_failed_proof() {
    let fx = fixture::set().unwrap();
    // The folder's own name is protected, as USERNAME=folder would be.
    let owner = fx.root().join("folder");
    mkdir(&owner);
    write(&owner.join("kept.txt"), b"owner bytes");
    let mut fakes = Fakes::new(true);
    fakes.write_into = Some(owner.join("kept.txt"));
    let ran = bench(&mut fakes, Some(&owner), &["--quick"]);
    assert_eq!(ran.exit, Exit::DataSafety);
    let json = ran.file("report.json");
    assert!(
        !json.contains("schema_version"),
        "the report must be redacted"
    );
    let html = ran.file("report.html");
    let console = String::from_utf8_lossy(&ran.console).into_owned();
    for text in [&json, &html, &console] {
        assert!(text.contains("DATA SAFETY: FAIL"), "{text}");
    }
    let mut everything = ran.console.clone();
    everything.extend(json.bytes());
    everything.extend(html.bytes());
    assert_no_leaks(&everything, &planted(&fx, &fakes)).unwrap();
    assert_eq!(fakes.run_roots_left(), 0);
}

#[test]
fn medium_a_panic_outside_any_phase_still_proves_the_folder_writes_both_reports_and_recycles() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(true);
    fakes.panic_in_measure_host = true;
    let ran = bench(&mut fakes, Some(fx.root()), &["--quick"]);
    assert_eq!(ran.exit, Exit::Failed);
    let run = ran.run_summary();
    assert_eq!(
        run["proof"]["state"], "equal",
        "the folder check after still runs"
    );
    let panicked = run["phases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|phase| phase["outcome"]["code"] == 101)
        .count();
    assert_eq!(panicked, 1);
    assert!(ran.file("report.html").contains("failed, code 101"));
    assert_eq!(fakes.run_roots_left(), 0);
    assert_eq!(fakes.recycler.calls.get(), 1);
}
