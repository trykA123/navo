use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use super::install_check::{
    Expected, Mismatch, Process, Running, check_image, check_running, is_mismatch,
};
use super::install_plan::{CopyPlan, FreshInstall, Step, run_install};

const NEW: &[u8] = b"navo build of today";
const OLD: &[u8] = b"navo build of 30 October";

struct Disk {
    _root: tempfile::TempDir,
    source: PathBuf,
    plan: CopyPlan,
}

fn disk(stale: Option<&[u8]>) -> Disk {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("navo.exe");
    std::fs::write(&source, NEW).unwrap();
    let plan = CopyPlan::new(&root.path().join("Program Files"), 7);
    std::fs::create_dir_all(&plan.directory).unwrap();
    if let Some(bytes) = stale {
        std::fs::write(&plan.executable, bytes).unwrap();
    }
    Disk {
        _root: root,
        source,
        plan,
    }
}

/// Runs the install on real files; the SCM steps succeed, `fault` fails one step by count,
/// and `meddle` runs at Start, where a stale process could swap the image.
fn install(
    disk: &Disk,
    exists: bool,
    fault: Option<usize>,
    meddle: impl Fn(&Path),
) -> io::Result<()> {
    let plan = &disk.plan;
    let mut fresh = FreshInstall::observe(plan, &[]);
    let mut count = 0;
    run_install(exists, exists, |step| {
        count += 1;
        if fault == Some(count) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        match step {
            Step::RecoverOld => fresh.recover_executable(plan),
            Step::Copy => plan.copy_from(&disk.source),
            Step::RenameOld => plan.rename_old(),
            Step::Replace => plan.replace(),
            Step::Restore => plan.restore(),
            Step::DiscardCopy => plan.discard_copy(),
            Step::RemoveFresh => fresh.remove_executable(plan),
            Step::DeleteOld => plan.delete_old(),
            Step::Verify => check_image(&disk.source, &plan.executable),
            Step::Start => {
                meddle(&plan.executable);
                Ok(())
            }
            _ => Ok(()),
        }
    })
}

fn image(disk: &Disk) -> Option<Vec<u8>> {
    std::fs::read(&disk.plan.executable).ok()
}

fn leftovers(disk: &Disk) -> Vec<String> {
    std::fs::read_dir(&disk.plan.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "navo.exe")
        .collect()
}

#[test]
fn an_install_over_a_stale_image_replaces_it() {
    let disk = disk(Some(OLD));
    install(&disk, true, None, |_| {}).unwrap();
    assert_eq!(image(&disk).as_deref(), Some(NEW));
    assert!(leftovers(&disk).is_empty(), "{:?}", leftovers(&disk));
}

#[test]
fn a_failed_replace_is_an_error_and_the_old_image_stays_whole() {
    let disk = disk(Some(OLD));
    let replace = 5;
    assert!(install(&disk, true, Some(replace), |_| {}).is_err());
    assert_eq!(image(&disk).as_deref(), Some(OLD));
}

#[test]
fn every_single_fault_ends_in_an_error_with_the_old_image_or_success_with_the_new() {
    // Calls 2 and 11 sweep old images; their failure is only a leftover.
    let calls = 11;
    for fault in 1..=calls {
        let disk = disk(Some(OLD));
        let result = install(&disk, true, Some(fault), |_| {});
        match result {
            Ok(()) => {
                assert!(fault == 2 || fault == calls, "fault {fault} was ignored");
                assert_eq!(image(&disk).as_deref(), Some(NEW), "fault {fault}");
            }
            Err(_) => {
                assert_eq!(image(&disk).as_deref(), Some(OLD), "fault {fault}");
                assert!(
                    !leftovers(&disk).iter().any(|name| name.ends_with(".tmp")),
                    "fault {fault} left a copy: {:?}",
                    leftovers(&disk)
                );
            }
        }
    }
}

#[test]
fn an_image_swapped_before_the_check_fails_the_install_and_rolls_back() {
    let disk = disk(Some(OLD));
    let result = install(&disk, true, None, |path| {
        if std::fs::read(path).unwrap() == NEW {
            std::fs::write(path, b"something else").unwrap();
        }
    });
    let error = result.unwrap_err();
    assert!(is_mismatch(&error), "{error}");
    assert_eq!(image(&disk).as_deref(), Some(OLD));
}

#[test]
fn an_exe_left_behind_by_remove_is_replaced_by_the_next_install() {
    let disk = disk(Some(OLD));
    let mapped_leftover = disk.plan.directory.join("navo.exe.99.old");
    std::fs::write(&mapped_leftover, OLD).unwrap();
    install(&disk, false, None, |_| {}).unwrap();
    assert_eq!(image(&disk).as_deref(), Some(NEW));
    assert!(leftovers(&disk).is_empty(), "{:?}", leftovers(&disk));
}

#[test]
fn a_fresh_install_that_fails_its_check_stops_the_service_it_started() {
    let mut calls = Vec::new();
    let result = run_install(false, false, |step| {
        calls.push(step);
        match step {
            Step::Verify => Err(io::Error::other(Mismatch::NotRunning)),
            _ => Ok(()),
        }
    });
    assert!(result.is_err());
    let verify = calls.iter().position(|&step| step == Step::Verify).unwrap();
    assert_eq!(calls[verify + 1], Step::Stop);
}

fn expected(started_after: u64) -> Expected<'static> {
    Expected {
        command: OsStr::new(r#""C:\Program Files\Navo\Index\navo.exe" --index-service run"#),
        image: Path::new(r"C:\Program Files\Navo\Index\navo.exe"),
        started_after,
    }
}

fn process(image: &str, created: u64) -> Option<Process> {
    Some(Process {
        image: PathBuf::from(image),
        created,
    })
}

#[test]
fn a_service_started_from_the_new_image_passes_regardless_of_case() {
    let running = Running {
        command: OsStr::new(r#""c:\program files\navo\index\NAVO.EXE" --index-service run"#),
        process: process(r"C:\Program Files\Navo\Index\navo.exe", 100),
    };
    assert!(check_running(&expected(100), &running).is_ok());
}

#[test]
fn each_way_the_service_can_miss_the_new_image_is_a_mismatch() {
    let command = expected(0).command;
    let cases = [
        (
            OsStr::new(r#""C:\Other\navo.exe" --index-service run"#),
            process(r"C:\Program Files\Navo\Index\navo.exe", 100),
        ),
        (command, None),
        (
            command,
            process(r"C:\Program Files\Navo\Index\navo.exe.7.old", 100),
        ),
        (
            command,
            process(r"C:\Program Files\Navo\Index\navo.exe", 99),
        ),
    ];
    let got: Vec<_> = cases
        .into_iter()
        .map(|(command, process)| check_running(&expected(100), &Running { command, process }))
        .collect();
    assert!(matches!(got[0], Err(Mismatch::Command(_))));
    assert!(matches!(got[1], Err(Mismatch::NotRunning)));
    assert!(matches!(got[2], Err(Mismatch::OtherImage(_))));
    assert!(matches!(got[3], Err(Mismatch::OldProcess)));
}
