use super::*;

#[test]
fn main_compat_unavailable_profiles_and_journals_allow_unlogged_jobs_and_creation() {
    let root = tempfile::tempdir().unwrap();
    let dirs = store(&root);
    dirs.ensure().unwrap();
    std::fs::create_dir(dirs.journal_file()).unwrap();
    for profile in [None, Some(&dirs)] {
        let (operations, journal) = super::super::operations(profile, &home(), false);
        assert!(journal.is_none());
        assert!(
            operations.write.is_none(),
            "main creates files without a journal hook"
        );
        let plan = rusty_core::OperationPlan {
            kind: rusty_core::OpKind::Copy,
            steps: vec![],
            totals: rusty_core::Totals {
                files: 0,
                folders: 0,
                bytes: 0,
            },
            conflicts: vec![],
            risks: vec![],
        };
        let mut log = (operations.log)(&plan).unwrap();
        log.check(rusty_ui::browser::LoggedOperation {
            plan: &plan,
            ops: &FakeFs::new(),
            cancel: &rusty_fs::CancelToken::new(),
            progress: &mut |_| {},
            retain_deletes: None,
        })
        .unwrap();
        let target = root.path().join(if profile.is_some() {
            "with-profile"
        } else {
            "no-profile"
        });
        let target = crate::startup::path_location(&target).unwrap();
        rusty_fs::LocalFs::new().create_file(&target).unwrap();
    }
}
