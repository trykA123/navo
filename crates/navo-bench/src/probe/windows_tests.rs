//! Probe tests against `examples/testee.rs`; cargo builds examples with the tests.

use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::time::Duration;

use super::keys::{Target, WM_CLOSE, WM_KEYDOWN, WM_KEYUP, post_keys};
use super::sysinfo::X64;
use super::win32::{self, Win32Windows};
use super::*;
use crate::guard::ScratchRoot;

const WINDOW_WAIT: Duration = Duration::from_secs(10);
const CLOSE_WAIT: Duration = Duration::from_secs(5);

fn testee() -> PathBuf {
    let deps = std::env::current_exe().unwrap();
    let exe = deps
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join("testee.exe");
    assert!(exe.is_file(), "build the testee example first");
    exe
}

fn start(root: &ScratchRoot, switches: &[&'static str]) -> Launched {
    let mut args: Vec<Arg<'_>> = switches.iter().map(|s| Arg::Switch(s)).collect();
    args.push(Arg::Path(
        PathFlag::Profile,
        std::path::Path::new("profile"),
    ));
    Launched::start(&testee(), root, &args).unwrap()
}

/// False, with a note, in a session without a visible window station (SSH, services): no
/// window can be shown there, so the window tests have nothing to observe.
#[allow(unsafe_code, reason = "reads this process's window-station flags")]
#[allow(clippy::print_stderr, reason = "the skip reason goes to the test log")]
fn has_desktop() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{
        GetProcessWindowStation, GetUserObjectInformationW, UOI_FLAGS, USEROBJECTFLAGS,
    };
    const WSF_VISIBLE: u32 = 1;
    let mut flags = USEROBJECTFLAGS::default();
    let size = u32::try_from(std::mem::size_of::<USEROBJECTFLAGS>()).unwrap();
    // SAFETY: the station handle needs no close; `flags` is a live buffer of `size` bytes.
    let ok = unsafe {
        GetUserObjectInformationW(
            GetProcessWindowStation(),
            UOI_FLAGS,
            (&raw mut flags).cast(),
            size,
            std::ptr::null_mut(),
        )
    };
    let visible = ok != 0 && flags.dwFlags & WSF_VISIBLE != 0;
    if !visible {
        eprintln!("skipped: this session has no interactive desktop");
    }
    visible
}

fn scratch() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, root)
}

#[test]
fn layout_and_messages_match_windows_sys() {
    if cfg!(target_pointer_width = "64") {
        assert_eq!(win32::layout(), X64);
    }
    use windows_sys::Win32::UI::WindowsAndMessaging as wm;
    assert_eq!(
        (WM_CLOSE, WM_KEYDOWN, WM_KEYUP),
        (wm::WM_CLOSE, wm::WM_KEYDOWN, wm::WM_KEYUP)
    );
}

#[test]
fn testee_runs_in_a_job_shows_a_window_and_counts_posted_keys() {
    if !has_desktop() {
        return;
    }
    let (_parent, root) = scratch();
    let mut launched = start(&root, &[]);
    assert!(win32::is_in_job(launched.child.as_raw_handle(), &launched.job).unwrap());
    let shown = launched.wait_for_window(WINDOW_WAIT).unwrap();
    assert!(shown > Duration::ZERO && shown < WINDOW_WAIT);
    let memory = launched.memory().unwrap();
    assert!(memory.private_bytes > 0 && memory.working_set_bytes > 0);
    launched.cpu_time().unwrap();
    assert!(!launched.thread_switches().unwrap().is_empty());
    let letter = Key::Letter(Letter::new('a').unwrap());
    launched
        .post_keys(&[Key::Down, Key::PageDown, letter, Key::Escape])
        .unwrap();
    let outcome = launched.close(CLOSE_WAIT).unwrap();
    assert_eq!(
        outcome,
        CloseOutcome {
            polite: true,
            exit_code: Some(104)
        }
    );
}

#[test]
fn late_window_time_includes_the_delay() {
    if !has_desktop() {
        return;
    }
    let (_parent, root) = scratch();
    let mut launched = start(&root, &["--late"]);
    assert!(launched.wait_for_window(WINDOW_WAIT).unwrap() >= Duration::from_millis(300));
    assert!(launched.close(CLOSE_WAIT).unwrap().polite);
}

#[test]
fn stubborn_process_is_terminated_after_the_deadline() {
    if !has_desktop() {
        return;
    }
    let (_parent, root) = scratch();
    let mut launched = start(&root, &["--stubborn"]);
    launched.wait_for_window(WINDOW_WAIT).unwrap();
    let outcome = launched.close(Duration::from_millis(300)).unwrap();
    assert!(!outcome.polite);
}

#[test]
fn no_window_times_out_and_keys_need_a_window() {
    let (_parent, root) = scratch();
    let mut launched = start(&root, &["--no-window"]);
    assert_eq!(launched.post_keys(&[Key::Down]), Err(ProbeError::NoWindow));
    let waited = launched.wait_for_window(Duration::from_millis(500));
    assert_eq!(waited, Err(ProbeError::NoWindow));
}

#[test]
fn keys_for_another_process_never_reach_this_window() {
    if !has_desktop() {
        return;
    }
    let (_parent, root) = scratch();
    let mut first = start(&root, &[]);
    let mut second = start(&root, &[]);
    first.wait_for_window(WINDOW_WAIT).unwrap();
    second.wait_for_window(WINDOW_WAIT).unwrap();
    let crossed = Target {
        window: first.window.unwrap(),
        pid: second.pid(),
    };
    assert_eq!(
        post_keys(&Win32Windows, crossed, &[Key::Down]),
        Err(ProbeError::WrongProcess)
    );
    assert_eq!(first.close(CLOSE_WAIT).unwrap().exit_code, Some(0));
    assert_eq!(second.close(CLOSE_WAIT).unwrap().exit_code, Some(0));
}

#[test]
fn dropping_the_launch_ends_the_process() {
    if !has_desktop() {
        return;
    }
    let (_parent, root) = scratch();
    let mut launched = start(&root, &[]);
    launched.wait_for_window(WINDOW_WAIT).unwrap();
    let pid = launched.pid();
    drop(launched);
    let gone = super::math::poll(CLOSE_WAIT, Duration::from_millis(20), || {
        win32::thread_switches(pid).map(|threads| threads.is_empty().then_some(()))
    });
    assert_eq!(gone, Ok(Some(())));
}
