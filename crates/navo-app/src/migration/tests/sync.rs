use super::super::{LenientSync, Outcome};
use super::fault_fs::{Fault, FaultFs};
use super::fixture::layouts;

#[test]
fn a_folder_sync_failure_does_not_fail_stage_or_commit() {
    for make in layouts() {
        let fixture = make();
        let original = fixture.tree();
        let fs = LenientSync::new(FaultFs::new(Fault::SyncDir), true);
        let outcome = fixture.run(&fs);
        assert!(matches!(outcome, Outcome::Done), "{outcome:?}");
        fixture.assert_one_navo_profile(&original);
        assert!(
            fs.into_failure().is_some(),
            "the sync error is kept for the log"
        );
    }
}

#[test]
fn a_strict_folder_sync_failure_still_stops_the_migration() {
    for make in layouts() {
        let fixture = make();
        let fs = LenientSync::new(FaultFs::new(Fault::SyncDir), false);
        assert!(matches!(fixture.run(&fs), Outcome::Failed(_)));
        assert!(fs.into_failure().is_none());
    }
}
