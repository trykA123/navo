use super::*;

fn bare(kind: rusty_core::OpKind, steps: Vec<rusty_core::Step>) -> rusty_core::OperationPlan {
    rusty_core::OperationPlan {
        kind,
        steps,
        totals: rusty_core::Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

fn checked(
    operations: &rusty_ui::browser::Operations,
    plan: &rusty_core::OperationPlan,
) -> Result<(), String> {
    let mut log = (operations.log)(plan)?;
    log.check(rusty_ui::browser::LoggedOperation {
        plan,
        ops: &rusty_fs::LocalFs::new(),
        cancel: &rusty_fs::CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
}

#[test]
fn safe_mode_without_a_profile_or_journal_runs_guarded_jobs_and_creation() {
    let root = tempfile::tempdir().unwrap();
    let dirs = store(&root);
    dirs.ensure().unwrap();
    std::fs::create_dir(dirs.journal_file()).unwrap();
    let files = root.path().join("files");
    std::fs::create_dir_all(files.join(".navo-retained-ab/payload")).unwrap();
    std::fs::write(files.join(".navo-retained-ab/payload/a"), b"kept").unwrap();
    let at = |path: &std::path::Path| crate::startup::path_location(path).unwrap();
    for profile in [None, Some(&dirs)] {
        let (operations, journal) = super::super::operations(
            profile,
            &home(),
            false,
            &(std::sync::Arc::new(rusty_fs::LocalFs::new())
                as std::sync::Arc<dyn rusty_fs::ProviderOps>),
        );
        assert!(journal.is_none());
        checked(&operations, &bare(rusty_core::OpKind::Copy, vec![])).unwrap();
        let reserved = [
            bare(
                rusty_core::OpKind::Delete,
                vec![rusty_core::Step::RemoveFolder {
                    location: at(&files.join(".navo-retained-ab")),
                }],
            ),
            bare(
                rusty_core::OpKind::Recycle,
                vec![rusty_core::Step::Recycle {
                    location: at(&files),
                }],
            ),
        ];
        for plan in &reserved {
            assert!(checked(&operations, plan).is_err(), "{plan:?}");
        }
        let write = operations.write.as_ref().unwrap();
        assert!(write(&at(&files.join(".navo-retained-ab/new"))).is_err());
        let target = root.path().join(if profile.is_some() {
            "with-profile"
        } else {
            "no-profile"
        });
        let _lease = write(&at(&target)).unwrap();
        rusty_fs::LocalFs::new().create_file(&at(&target)).unwrap();
    }
    assert_eq!(
        std::fs::read(files.join(".navo-retained-ab/payload/a")).unwrap(),
        b"kept"
    );
}

#[test]
fn safe_mode_without_a_journal_protects_the_profile_folder() {
    let root = tempfile::tempdir().unwrap();
    let dirs = store(&root);
    dirs.ensure().unwrap();
    std::fs::create_dir(dirs.journal_file()).unwrap();
    let (operations, journal) = super::super::operations(
        Some(&dirs),
        &home(),
        false,
        &(std::sync::Arc::new(rusty_fs::LocalFs::new())
            as std::sync::Arc<dyn rusty_fs::ProviderOps>),
    );
    assert!(journal.is_none());
    let profile_delete = bare(
        rusty_core::OpKind::Delete,
        vec![rusty_core::Step::RemoveFolder {
            location: crate::startup::path_location(&dirs.data).unwrap(),
        }],
    );
    assert_eq!(
        checked(&operations, &profile_delete).unwrap_err(),
        rusty_l10n::tr("job-profile-protected")
    );
    assert!(dirs.data.exists());
}
