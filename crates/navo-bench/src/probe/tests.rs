use std::cell::{Cell, RefCell};
use std::path::Path;
use std::time::Duration;

use super::args::{build, valid_switch};
use super::keys::{
    Message, Target, WM_KEYDOWN, WM_KEYUP, WindowId, WindowInfo, WindowOs, key_lparam, main_window,
    post_keys, post_verified,
};
use super::math::{poll, ticks};
use super::sysinfo::{X64, threads_of};
use super::*;
use crate::guard::ScratchRoot;

const NAVO: u32 = 4242;
const OTHER: u32 = 777;
const WINDOW: WindowId = WindowId(0x1000);

/// Fake windows: `WINDOW` belongs to `NAVO` for the first `honest_checks` checks, then to `OTHER`.
struct FakeOs {
    honest_checks: usize,
    checks: Cell<usize>,
    posted: RefCell<Vec<(WindowId, Message)>>,
}

impl FakeOs {
    fn new(honest_checks: usize) -> Self {
        Self {
            honest_checks,
            checks: Cell::new(0),
            posted: RefCell::new(Vec::new()),
        }
    }
}

impl WindowOs for FakeOs {
    fn window_pid(&self, window: WindowId) -> Option<u32> {
        let seen = self.checks.get();
        self.checks.set(seen + 1);
        (window == WINDOW).then_some(if seen < self.honest_checks {
            NAVO
        } else {
            OTHER
        })
    }

    fn post(&self, window: WindowId, message: Message) -> Result<(), ProbeError> {
        self.posted.borrow_mut().push((window, message));
        Ok(())
    }

    fn scan_code(&self, virtual_key: u16) -> u8 {
        u8::try_from(virtual_key % 0x60).unwrap()
    }
}

fn target(window: WindowId) -> Target {
    Target { window, pid: NAVO }
}

fn letter(ch: char) -> Key {
    Key::Letter(Letter::new(ch).unwrap())
}

#[test]
fn keys_reach_the_target_window_as_down_then_up_pairs() {
    let os = FakeOs::new(usize::MAX);
    post_keys(&os, target(WINDOW), &[Key::Down, letter('q'), Key::Escape]).unwrap();
    let posted = os.posted.borrow();
    let ids: Vec<(u32, usize)> = posted.iter().map(|(_, m)| (m.id, m.wparam)).collect();
    assert_eq!(
        ids,
        [
            (WM_KEYDOWN, 0x28),
            (WM_KEYUP, 0x28),
            (WM_KEYDOWN, 0x51),
            (WM_KEYUP, 0x51),
            (WM_KEYDOWN, 0x1B),
            (WM_KEYUP, 0x1B)
        ]
    );
    assert!(posted.iter().all(|(window, _)| *window == WINDOW));
    assert_eq!(os.checks.get(), 6, "one PID check per post");
}

#[test]
fn no_key_is_posted_to_a_window_of_another_process() {
    let os = FakeOs::new(0);
    let result = post_keys(&os, target(WINDOW), &[Key::Down, Key::PageDown]);
    assert_eq!(result, Err(ProbeError::WrongProcess));
    assert!(os.posted.borrow().is_empty());
}

#[test]
fn posting_stops_the_moment_the_window_changes_owner() {
    let os = FakeOs::new(3);
    let result = post_keys(
        &os,
        target(WINDOW),
        &[Key::Down, Key::PageDown, letter('a')],
    );
    assert_eq!(result, Err(ProbeError::WrongProcess));
    assert_eq!(os.posted.borrow().len(), 3);
    assert_eq!(os.checks.get(), 4);
}

#[test]
fn a_vanished_window_gets_nothing() {
    let os = FakeOs::new(usize::MAX);
    let gone = Target {
        window: WindowId(0x2000),
        pid: NAVO,
    };
    let close = Message {
        id: 0x10,
        wparam: 0,
        lparam: 0,
    };
    assert_eq!(
        post_verified(&os, gone, close),
        Err(ProbeError::WrongProcess)
    );
    assert!(os.posted.borrow().is_empty());
}

#[test]
fn main_window_is_the_first_visible_unowned_window_of_the_pid() {
    let info = |id, pid, visible, owned| WindowInfo {
        id: WindowId(id),
        pid,
        visible,
        owned,
    };
    let windows = [
        info(1, OTHER, true, false),
        info(2, NAVO, false, false),
        info(3, NAVO, true, true),
        info(4, NAVO, true, false),
        info(5, NAVO, true, false),
    ];
    assert_eq!(main_window(&windows, NAVO), Some(WindowId(4)));
    assert_eq!(main_window(&windows[..4], 9), None);
    assert_eq!(main_window(&windows[..3], NAVO), None);
}

#[test]
fn key_lparam_sets_scan_extended_and_transition_bits() {
    assert_eq!(key_lparam(0x50, true, false), 0x0150_0001);
    let up = key_lparam(0x1E, false, true);
    assert_eq!(up as u32, 0xC01E_0001);
    assert!(up < 0, "bit 31 makes LPARAM negative, as Win32 delivers it");
}

#[test]
fn letters_are_ascii_letters_and_digits_only() {
    assert_eq!(Letter::new('a'), Letter::new('A'));
    assert!(Letter::new('7').is_some());
    for refused in [' ', '-', '\n', 'é', 'ß', '\u{1b}'] {
        assert_eq!(Letter::new(refused), None, "{refused:?}");
    }
    assert_eq!(letter('z'), Key::Letter(Letter::new('Z').unwrap()));
}

#[test]
fn switch_deltas_cover_new_reused_and_exited_threads() {
    let at = |thread_id, switches| ThreadSwitches {
        thread_id,
        switches,
    };
    let before = [at(1, 100), at(2, 50), at(3, 900), at(4, 7)];
    let after = [at(1, 160), at(2, 50), at(3, 20), at(5, 12)];
    let deltas = switch_deltas(&before, &after);
    let pairs: Vec<(u32, u64)> = deltas.iter().map(|d| (d.thread_id, d.switches)).collect();
    assert_eq!(pairs, [(1, 60), (3, 20), (5, 12), (2, 0)]);
    let total: u64 = deltas.iter().map(|d| d.switches).sum();
    assert_eq!(per_second(total, Duration::from_secs(46)), Some(2.0));
    assert_eq!(per_second(5, Duration::ZERO), None);
}

#[test]
fn cpu_time_joins_filetime_halves_and_never_goes_negative() {
    assert_eq!(ticks(0xFFFF_FFFF, 1), 0x1_FFFF_FFFF);
    let earlier = CpuTime::from_filetimes((10_000, 0), (20_000, 0));
    let later = CpuTime::from_filetimes((30_000, 0), (25_000, 0));
    assert!((earlier.total_ms() - 3.0).abs() < 1e-9);
    assert!((later.ms_since(earlier) - 2.5).abs() < 1e-9);
    assert!(earlier.ms_since(later).abs() < 1e-9);
}

#[test]
fn poll_returns_the_value_times_out_and_passes_errors() {
    let mut calls = 0;
    let found = poll(Duration::from_secs(5), Duration::from_millis(1), || {
        calls += 1;
        Ok::<_, ProbeError>((calls == 3).then_some(calls))
    });
    assert_eq!(found, Ok(Some(3)));
    let none = poll(Duration::from_millis(5), Duration::from_millis(1), || {
        Ok::<Option<()>, ProbeError>(None)
    });
    assert_eq!(none, Ok(None));
    let failed = poll(Duration::from_secs(5), Duration::from_millis(1), || {
        Err::<Option<()>, _>(ProbeError::Exited)
    });
    assert_eq!(failed, Err(ProbeError::Exited));
}

fn process_entry(pid: u64, threads: &[(u64, u32)], last: bool) -> Vec<u8> {
    let size = X64.process_size + threads.len() * X64.thread_size;
    let mut entry = vec![0_u8; size];
    let next = if last {
        0
    } else {
        u32::try_from(size).unwrap()
    };
    entry[0..4].copy_from_slice(&next.to_ne_bytes());
    let count = u32::try_from(threads.len()).unwrap();
    entry[4..8].copy_from_slice(&count.to_ne_bytes());
    entry[X64.process_id..X64.process_id + 8].copy_from_slice(&pid.to_ne_bytes());
    for (index, (tid, switches)) in threads.iter().enumerate() {
        let base = X64.process_size + index * X64.thread_size;
        let id_at = base + X64.thread_id;
        entry[id_at..id_at + 8].copy_from_slice(&tid.to_ne_bytes());
        let sw_at = base + X64.switches;
        entry[sw_at..sw_at + 4].copy_from_slice(&switches.to_ne_bytes());
    }
    entry
}

#[test]
fn sysinfo_walk_finds_the_threads_of_one_process() {
    let mut buffer = process_entry(0, &[(0, 99)], false);
    buffer.extend(process_entry(u64::from(OTHER), &[(8, 1)], false));
    buffer.extend(process_entry(u64::from(NAVO), &[(10, 5), (11, 70)], true));
    let threads = threads_of(&buffer, &X64, NAVO).unwrap();
    let expected = [
        ThreadSwitches {
            thread_id: 10,
            switches: 5,
        },
        ThreadSwitches {
            thread_id: 11,
            switches: 70,
        },
    ];
    assert_eq!(threads, expected);
    assert_eq!(threads_of(&buffer, &X64, 31337).unwrap(), []);
}

#[test]
fn sysinfo_walk_refuses_truncated_or_inconsistent_buffers() {
    let whole = process_entry(u64::from(NAVO), &[(10, 5), (11, 70)], true);
    let cut = &whole[..whole.len() - 1];
    assert_eq!(threads_of(cut, &X64, NAVO), Err(ProbeError::Malformed));
    let mut lying = process_entry(1, &[], false);
    lying[0..4].copy_from_slice(&0x10_0000_u32.to_ne_bytes());
    assert_eq!(threads_of(&lying, &X64, NAVO), Err(ProbeError::Malformed));
    let mut many = whole.clone();
    many[4..8].copy_from_slice(&u32::MAX.to_ne_bytes());
    assert_eq!(threads_of(&many, &X64, NAVO), Err(ProbeError::Malformed));
    assert_eq!(threads_of(&[], &X64, NAVO), Err(ProbeError::Malformed));
}

#[test]
fn switches_are_lowercase_flags_only() {
    for good in ["--no-retention", "--late", "--a"] {
        assert!(valid_switch(good), "{good}");
    }
    for bad in [
        "", "--", "---x", "-x", "x", "--Late", "--a b", "--a=1", "--a/b", "--ä",
    ] {
        assert!(!valid_switch(bad), "{bad:?}");
    }
}

#[test]
fn path_arguments_are_absolute_inside_the_run_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    let args = [
        Arg::Path(PathFlag::Profile, Path::new("profiles/1")),
        Arg::Switch("--late"),
        Arg::Path(PathFlag::Metrics, Path::new("metrics-1.json")),
    ];
    let built = build(&root, &args).unwrap();
    assert_eq!(built.len(), 5);
    assert_eq!(built[0], "--profile");
    assert_eq!(built[1], root.path().join("profiles/1"));
    assert_eq!(built[2], "--late");
    assert_eq!(built[3], "--metrics");
    assert!(Path::new(&built[4]).starts_with(root.path()));
}

#[test]
fn path_arguments_outside_the_run_root_are_refused() {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    let outside = parent.path().join("elsewhere");
    for rel in [
        Path::new(""),
        Path::new("."),
        Path::new("../x"),
        Path::new("a/../../b"),
        outside.as_path(),
    ] {
        let result = build(&root, &[Arg::Path(PathFlag::Left, rel)]);
        assert_eq!(result, Err(ProbeError::NotRelative), "{rel:?}");
    }
    let switch = build(&root, &[Arg::Switch("--Left")]);
    assert_eq!(switch, Err(ProbeError::BadSwitch));
}

#[test]
fn the_bench_command_comes_first_and_read_only_folders_may_lie_outside_the_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    let elsewhere = tempfile::tempdir_in(parent.path()).unwrap();
    let owner = elsewhere.path().to_path_buf();
    let args = [
        Arg::Bench,
        Arg::ReadOnly(PathFlag::Folder, &owner),
        Arg::Path(PathFlag::Out, Path::new("out/b1.json")),
    ];
    let built = build(&root, &args).unwrap();
    assert_eq!(built[0], "bench");
    assert_eq!(
        (built[1].as_os_str(), built[2].as_os_str()),
        ("--folder".as_ref(), owner.as_os_str())
    );
    assert_eq!(built[3], "--out");
    assert!(Path::new(&built[4]).starts_with(root.path()));
    let late = build(&root, &[Arg::Switch("--late"), Arg::Bench]);
    assert_eq!(late, Err(ProbeError::BadReadOnly));
}

#[test]
fn read_only_folders_are_absolute_outside_the_root_and_on_read_flags_only() {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    let elsewhere = tempfile::tempdir_in(parent.path()).unwrap();
    let outside = elsewhere.path().to_path_buf();
    let inside = root.path().join("fixtures");
    let climbing = outside.join("..").join("x");
    for (flag, path) in [
        (PathFlag::Left, Path::new("relative")),
        (PathFlag::Left, inside.as_path()),
        (PathFlag::Left, root.path()),
        (PathFlag::Left, climbing.as_path()),
        (PathFlag::Out, outside.as_path()),
        (PathFlag::Profile, outside.as_path()),
        (PathFlag::Metrics, outside.as_path()),
        (PathFlag::Trace, outside.as_path()),
    ] {
        let result = build(&root, &[Arg::ReadOnly(flag, path)]);
        assert_eq!(result, Err(ProbeError::BadReadOnly), "{flag:?} {path:?}");
    }
    for flag in [PathFlag::Left, PathFlag::Right, PathFlag::Folder] {
        assert!(build(&root, &[Arg::ReadOnly(flag, &outside)]).is_ok());
    }
}

#[cfg(not(windows))]
#[test]
fn start_checks_arguments_then_refuses_off_windows() {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    let exe = Path::new("navo");
    let bad = Launched::start(exe, &root, &[Arg::Path(PathFlag::Trace, Path::new("../t"))]);
    assert_eq!(bad.err(), Some(ProbeError::NotRelative));
    let good = Launched::start(
        exe,
        &root,
        &[Arg::Path(PathFlag::Trace, Path::new("t.jsonl"))],
    );
    assert_eq!(good.err(), Some(ProbeError::Unsupported));
}

#[test]
fn errors_print_fixed_tags_only() {
    let errors = [
        ProbeError::Os {
            step: Step::Post,
            code: 5,
        },
        ProbeError::Spawn(std::io::ErrorKind::NotFound),
        ProbeError::Guard(crate::guard::GuardError::OutsideRoot(
            crate::guard::PathRole::ScratchRoot,
        )),
        ProbeError::WrongProcess,
    ];
    let text: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(
        text,
        [
            "probe:Post-os-5",
            "probe:spawn-NotFound",
            "probe:argument-scratch-root: outside-root",
            "probe:wrong-process",
        ]
    );
}
