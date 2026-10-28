use std::path::{Path, PathBuf};

use super::*;
use rusty_fs::{JobContext, JobOutcome, run_job};

struct Rig {
    _root: tempfile::TempDir,
    files: PathBuf,
    dirs: StoreDirs,
    journal: SharedJournal,
}

fn rig() -> Rig {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(&root.path().join("profile"));
    dirs.ensure().unwrap();
    let journal = super::super::super::open(&dirs.journal_file()).unwrap();
    let files = root.path().join("files");
    std::fs::create_dir_all(files.join(".navo-retained-ab/payload")).unwrap();
    std::fs::write(files.join(".navo-retained-ab/payload/a"), b"kept").unwrap();
    std::fs::write(files.join("ordinary.txt"), b"old").unwrap();
    std::fs::write(files.join("new.txt"), b"new").unwrap();
    std::fs::create_dir_all(root.path().join("elsewhere")).unwrap();
    Rig {
        _root: root,
        files,
        dirs,
        journal,
    }
}

fn at(path: &Path) -> Location {
    crate::startup::path_location(path).unwrap()
}

fn bare(kind: OpKind, steps: Vec<Step>) -> OperationPlan {
    OperationPlan {
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

fn delete(path: &Path) -> OperationPlan {
    let ops = LocalFs::new();
    plan(&PlanRequest {
        kind: OpKind::Delete,
        items: scan(&ops, &[at(path)], &CancelToken::new()).unwrap(),
        target: None,
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    })
}

fn rename(kind: OpKind, source: &Path, target: &Path) -> OperationPlan {
    bare(
        kind,
        vec![Step::Rename {
            source: at(source),
            target: at(target),
        }],
    )
}

fn refusal(rig: &Rig, plan: &OperationPlan) -> String {
    let ops = LocalFs::new();
    match profile_factory(rig.journal.clone(), rig.dirs.clone())(plan) {
        Err(error) => error,
        Ok(mut log) => log
            .check(LoggedOperation {
                plan,
                ops: &ops,
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
                waiting: &mut |_| {},
                retain_deletes: None,
            })
            .unwrap_err(),
    }
}

fn run(rig: &Rig, plan: &OperationPlan) {
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let mut log = profile_factory(rig.journal.clone(), rig.dirs.clone())(plan).unwrap();
    log.check(LoggedOperation {
        plan,
        ops: &ops,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: &ops,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, &ops).unwrap();
}

fn reserved_error(path: &Path) -> String {
    rusty_fs::mutation_location(&at(path))
        .unwrap_err()
        .message()
}

fn first_reserved_error(plan: &OperationPlan) -> String {
    plan.steps
        .iter()
        .flat_map(|step| match step {
            Step::Rename { source, target } => vec![source, target],
            Step::CopyFile { target, .. } | Step::MakeFolder { target } => vec![target],
            Step::MakeLink { at, .. } => vec![at],
            Step::Recycle { location }
            | Step::RemoveFile { location }
            | Step::RemoveLink { location }
            | Step::RemoveFolder { location } => vec![location],
        })
        .find_map(|path| rusty_fs::mutation_location(path).err())
        .unwrap()
        .message()
}

fn container_kept(rig: &Rig) {
    assert_eq!(
        std::fs::read(rig.files.join(".navo-retained-ab/payload/a")).unwrap(),
        b"kept"
    );
}

#[test]
fn safe_mode_refuses_every_mutation_of_a_retained_container() {
    let rig = rig();
    let container = rig.files.join(".navo-retained-ab");
    let elsewhere = rig.files.parent().unwrap().join("elsewhere");
    let plans = [
        delete(&container),
        delete(&container.join("payload/a")),
        rename(OpKind::Move, &container, &elsewhere.join("moved")),
        rename(OpKind::Rename, &container, &rig.files.join("renamed")),
        bare(
            OpKind::Copy,
            vec![Step::CopyFile {
                source: at(&rig.files.join("new.txt")),
                target: at(&container.join("payload/a")),
                size: 3,
            }],
        ),
        bare(
            OpKind::Recycle,
            vec![Step::Recycle {
                location: at(&container),
            }],
        ),
    ];
    for plan in &plans {
        assert_eq!(refusal(&rig, plan), first_reserved_error(plan), "{plan:?}");
    }
    container_kept(&rig);
}

#[test]
fn safe_mode_refuses_moving_or_recycling_a_folder_holding_a_container() {
    let rig = rig();
    let elsewhere = rig.files.parent().unwrap().join("elsewhere");
    let expected = reserved_error(&rig.files.join(".navo-retained-ab"));
    let plans = [
        rename(OpKind::Move, &rig.files, &elsewhere.join("files")),
        rename(
            OpKind::Rename,
            &rig.files,
            &rig.files.with_file_name("papers"),
        ),
        bare(
            OpKind::Recycle,
            vec![Step::Recycle {
                location: at(&rig.files),
            }],
        ),
    ];
    for plan in &plans {
        assert_eq!(refusal(&rig, plan), expected, "{plan:?}");
    }
    container_kept(&rig);
}

#[test]
fn safe_mode_refuses_mutations_of_the_profile_folder() {
    let rig = rig();
    let profile = rig.dirs.data.parent().unwrap().to_path_buf();
    let elsewhere = rig.files.parent().unwrap().join("elsewhere");
    let plans = [
        delete(&rig.dirs.data),
        delete(&rig.dirs.journal_file()),
        rename(OpKind::Move, &rig.dirs.config, &elsewhere.join("config")),
        rename(OpKind::Rename, &profile, &profile.with_file_name("other")),
        bare(
            OpKind::Recycle,
            vec![Step::Recycle {
                location: at(&profile),
            }],
        ),
        bare(
            OpKind::Copy,
            vec![Step::CopyFile {
                source: at(&rig.files.join("new.txt")),
                target: at(&rig.dirs.journal_file()),
                size: 3,
            }],
        ),
    ];
    for plan in &plans {
        assert_eq!(
            refusal(&rig, plan),
            rusty_l10n::tr("job-profile-protected"),
            "{plan:?}"
        );
    }
    assert!(rig.dirs.journal_file().exists());
}

#[test]
fn safe_mode_compares_a_missing_profile_folder_as_written() {
    let rig = rig();
    std::fs::rename(&rig.dirs.config, rig.files.with_file_name("parked")).unwrap();
    let new = at(&rig.dirs.config.join("new.txt"));
    assert_eq!(
        claim_write(&rig.dirs, &new, false).unwrap_err(),
        rusty_l10n::tr("job-profile-protected")
    );
    assert!(claim_write(&rig.dirs, &at(&rig.files.join("ordinary.txt")), false).is_ok());
    assert_eq!(
        refusal(&rig, &delete(&rig.dirs.data)),
        rusty_l10n::tr("job-profile-protected")
    );
}

#[test]
fn safe_mode_file_creation_refuses_containers_and_the_profile() {
    let rig = rig();
    let write = write_factory(rig.dirs.clone(), false);
    for path in [
        rig.files.join(".navo-retained-ab/payload/new"),
        rig.files.join(".navo-held-1-2"),
        rig.dirs.data.join("new"),
    ] {
        assert!(
            claim_write(&rig.dirs, &at(&path), false).is_err(),
            "{path:?}"
        );
        assert!(write(&at(&path)).is_err(), "{path:?}");
    }
    let ordinary = at(&rig.files.join("made.txt"));
    assert!(claim_write(&rig.dirs, &ordinary, false).unwrap().is_none());
    let _lease = write(&ordinary).unwrap();
    LocalFs::new().create_file(&ordinary).unwrap();
    container_kept(&rig);
}

#[test]
fn safe_mode_still_overwrites_and_moves_ordinary_files() {
    let rig = rig();
    let elsewhere = rig.files.parent().unwrap().join("elsewhere");
    std::fs::write(elsewhere.join("ordinary.txt"), b"new").unwrap();
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let items = scan(&ops, &[at(&elsewhere.join("ordinary.txt"))], &cancel).unwrap();
    let overwrite = plan(&PlanRequest {
        kind: OpKind::Copy,
        existing: rusty_fs::existing_at(&ops, &items, &at(&rig.files), &cancel).unwrap(),
        items,
        target: Some(at(&rig.files)),
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: vec![],
    });
    run(&rig, &overwrite);
    assert_eq!(
        std::fs::read(rig.files.join("ordinary.txt")).unwrap(),
        b"new"
    );
    run(
        &rig,
        &rename(
            OpKind::Move,
            &rig.files.join("ordinary.txt"),
            &elsewhere.join("moved.txt"),
        ),
    );
    assert_eq!(std::fs::read(elsewhere.join("moved.txt")).unwrap(), b"new");
    std::fs::create_dir(rig.files.with_file_name("folder")).unwrap();
    std::fs::write(rig.files.with_file_name("folder").join("x"), b"x").unwrap();
    run(
        &rig,
        &rename(
            OpKind::Rename,
            &rig.files.with_file_name("folder"),
            &rig.files.with_file_name("renamed"),
        ),
    );
    assert_eq!(
        std::fs::read(rig.files.with_file_name("renamed").join("x")).unwrap(),
        b"x"
    );
    container_kept(&rig);
}
