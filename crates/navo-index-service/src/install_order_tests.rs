use super::super::{CopyPlan, FreshInstall, Step, run_install};
use std::io;
use std::path::PathBuf;

const NEW: &[u8] = b"navo-index of today";
const OLD: &[u8] = b"navo-index of last week";

/// The install on real files with the SCM steps recorded: `previous` is the image already in
/// `Index` (none when migrating from `navo.exe`), `fault` fails the first run of that step.
struct World {
    _root: tempfile::TempDir,
    source: PathBuf,
    plan: CopyPlan,
    calls: Vec<Step>,
}

fn world(previous: Option<&[u8]>) -> World {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("navo-index.exe");
    std::fs::write(&source, NEW).unwrap();
    let plan = CopyPlan::new(&root.path().join("Program Files"), 7);
    std::fs::create_dir_all(&plan.directory).unwrap();
    if let Some(bytes) = previous {
        std::fs::write(&plan.executable, bytes).unwrap();
    }
    World {
        _root: root,
        source,
        plan,
        calls: Vec::new(),
    }
}

impl World {
    fn install(&mut self, fault: Option<Step>) -> io::Result<Option<io::Error>> {
        let (plan, source, calls) = (&self.plan, &self.source, &mut self.calls);
        let mut fresh = FreshInstall::observe(plan, &[]);
        run_install(true, true, |step| {
            let failing = fault == Some(step) && !calls.contains(&step);
            calls.push(step);
            if failing {
                return Err(io::Error::other(format!("{step:?} failed")));
            }
            match step {
                Step::RecoverOld => fresh.recover_executable(plan),
                Step::Copy => plan.copy_from(source),
                Step::RenameOld => plan.rename_old(),
                Step::Replace => plan.replace(),
                Step::Restore => plan.restore(),
                Step::DiscardCopy => plan.discard_copy(),
                Step::RemoveFresh => fresh.remove_executable(plan),
                Step::DeleteOld => plan.delete_old(),
                _ => Ok(()),
            }
        })
    }

    fn image(&self) -> Option<Vec<u8>> {
        std::fs::read(&self.plan.executable).ok()
    }

    fn files(&self) -> Vec<String> {
        std::fs::read_dir(&self.plan.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn position(&self, step: Step) -> usize {
        self.calls.iter().position(|&call| call == step).unwrap()
    }
}

#[test]
fn migration_and_reinstall_both_run_the_decided_order_and_delete_legacy_last() {
    use Step::*;
    for previous in [None, Some(OLD)] {
        let mut world = world(previous);
        assert!(world.install(None).unwrap().is_none());
        assert_eq!(
            world.calls,
            [
                RecoverOld,
                DeleteOld,
                Copy,
                Stop,
                RenameOld,
                Replace,
                BackupSettings,
                Configure,
                Start,
                Verify,
                Answer,
                DeleteOld,
                DeleteLegacy
            ],
            "previous {previous:?}"
        );
        assert_eq!(world.image().as_deref(), Some(NEW));
        assert_eq!(world.files(), ["navo.exe"]);
    }
}

#[test]
fn a_failure_from_stop_to_answer_restores_the_previous_state_and_never_deletes_legacy() {
    use Step::*;
    let failing = [
        Stop,
        RenameOld,
        Replace,
        BackupSettings,
        Configure,
        Start,
        Verify,
        Answer,
    ];
    for previous in [None, Some(OLD)] {
        for fault in failing {
            let mut world = world(previous);
            let error = world.install(Some(fault)).unwrap_err();
            let case = format!("{fault:?} with previous {previous:?}");
            assert!(error.to_string().starts_with(&format!("{fault:?} failed")));
            assert_eq!(world.image().as_deref(), previous, "{case}");
            assert!(world.files().len() <= 1, "{case}: {:?}", world.files());
            assert!(!world.calls.contains(&DeleteLegacy), "{case}");
            let rollback = &world.calls[world.position(fault) + 1..];
            if world.calls[..world.position(fault)].contains(&Replace) {
                assert!(rollback.contains(&RestoreConfiguration), "{case}");
            }
            assert_eq!(rollback.last(), Some(&Start), "{case} restarts the service");
        }
    }
}

#[test]
fn a_replacement_that_ran_is_stopped_before_its_image_is_removed() {
    use Step::*;
    let mut world = world(None);
    world.install(Some(Answer)).unwrap_err();
    let rollback = &world.calls[world.position(Answer) + 1..];
    assert_eq!(
        rollback,
        [
            Stop,
            RestoreSettings,
            Restore,
            RemoveFresh,
            RestoreConfiguration,
            Start
        ]
    );
    assert_eq!(world.image(), None);
}

#[test]
fn a_failed_legacy_delete_still_succeeds_with_a_warning() {
    let mut world = world(Some(OLD));
    let warning = world.install(Some(Step::DeleteLegacy)).unwrap();
    let warning = warning.expect("the leftover legacy image is reported");
    assert!(warning.to_string().contains("DeleteLegacy failed"));
    assert_eq!(world.image().as_deref(), Some(NEW));
    assert!(!world.calls.contains(&Step::Restore));
}
