#[cfg(test)]
mod fixture;

#[cfg(test)]
mod cli_process {
    use std::fs;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;

    #[cfg(unix)]
    use navo_git::CliOptions;
    use navo_git::{Cancel, CliGit, GitError, ProcessOptions, ProcessOutcome};

    use super::fixture::Fixture;

    #[cfg(unix)]
    fn cli(fixture: &Fixture, body: &str) -> CliGit {
        let script = format!(
            "#!/bin/sh\nif [ \"$3\" = gs2-helper ]; then\ncd \"$2\" || exit 1\n{body}\nexit 0\nfi\nexec git \"$@\"\n"
        );
        CliGit::with_options(
            fixture.repo(),
            &CliOptions {
                git: Some(fixture.script("git-process", &script)),
                ..CliOptions::default()
            },
        )
        .unwrap()
    }

    #[cfg(windows)]
    fn cli(fixture: &Fixture, body: &str) -> CliGit {
        fixture.git_ok(&["config", "alias.gs2-helper", &format!("!{body}")]);
        CliGit::new(fixture.repo()).unwrap()
    }

    fn fixture() -> Fixture {
        let fixture = Fixture::new().unwrap();
        fixture.init(&["-b", "main"]);
        fixture
    }

    fn options(cap: usize) -> ProcessOptions {
        ProcessOptions {
            stdout_cap: cap,
            timeout: Duration::from_secs(5),
        }
    }

    #[cfg(target_os = "linux")]
    fn thread_count() -> usize {
        fs::read_dir("/proc/self/task").unwrap().count()
    }

    /// The kernel drops a joined thread from /proc a moment after join returns.
    #[cfg(target_os = "linux")]
    fn settled_thread_count(before: usize) -> usize {
        let deadline = Instant::now() + Duration::from_secs(5);
        while thread_count() != before && Instant::now() < deadline {
            thread::yield_now();
        }
        thread_count()
    }

    fn wait_file(fixture: &Fixture, name: &str) -> String {
        let started = Instant::now();
        loop {
            if let Ok(value) = fs::read_to_string(fixture.repo().join(name))
                && !value.trim().is_empty()
            {
                return value.trim().to_owned();
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "helper did not start"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[cfg(unix)]
    fn running(pid: &str) -> bool {
        #[cfg(target_os = "linux")]
        if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
            return !stat.rsplit_once(") ").unwrap().1.starts_with('Z');
        }
        Command::new("kill")
            .args(["-0", pid])
            .output()
            .unwrap()
            .status
            .success()
    }

    #[cfg(unix)]
    fn assert_reaped(pid: &str) {
        assert!(
            !Command::new("kill")
                .args(["-0", pid])
                .output()
                .unwrap()
                .status
                .success(),
            "the direct child must be reaped, including its zombie entry"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancel_mid_run_kills_child_and_grandchild_within_100ms() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "printf payload\necho $$ > parent.pid\nsleep 20 &\necho $! > child.pid\nwait",
        );
        let cancel = Cancel::default();
        thread::scope(|scope| {
            let handle =
                scope.spawn(|| cli.run_cancellable(&["gs2-helper"], &cancel, &options(65536)));
            let parent = wait_file(&fixture, "parent.pid");
            let child = wait_file(&fixture, "child.pid");
            assert!(running(&parent) && running(&child));
            let started = Instant::now();
            cancel.cancel();
            let output = handle.join().unwrap().unwrap();
            assert!(started.elapsed() < Duration::from_millis(100));
            assert_eq!(output.outcome, ProcessOutcome::Cancelled);
            assert_eq!(output.stdout, b"payload");
            assert_reaped(&parent);
            assert!(!running(&child));
        });
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_stalled_tree_and_reaps_parent() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "echo $$ > parent.pid\nsleep 20 &\necho $! > child.pid\nwait",
        );
        let output = cli
            .run_cancellable(
                &["gs2-helper"],
                &Cancel::default(),
                &ProcessOptions {
                    timeout: Duration::from_millis(100),
                    ..options(65536)
                },
            )
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::TimedOut);
        assert_reaped(&wait_file(&fixture, "parent.pid"));
        assert!(!running(&wait_file(&fixture, "child.pid")));
    }

    /// A pause held before the call refuses it without spawning.
    #[test]
    fn a_live_pause_refuses_run_cancellable() {
        let fixture = fixture();
        let cli = cli(&fixture, "echo started > parent.pid");
        let pause = navo_git::cli::stop_under(fixture.repo(), Duration::from_millis(100)).unwrap();
        let started = Instant::now();
        let result = cli.run_cancellable(&["gs2-helper"], &Cancel::default(), &options(65536));
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "the refusal took {:?}",
            started.elapsed()
        );
        assert!(matches!(result, Err(GitError::Paused)));
        assert!(!fixture.repo().join("parent.pid").exists());
        drop(pause);
    }

    #[cfg(unix)]
    #[test]
    fn stop_under_pauses_a_running_cancellable_call() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "printf payload\necho $$ > parent.pid\nsleep 20 &\necho $! > child.pid\nwait",
        );
        let cancel = Cancel::default();
        thread::scope(|scope| {
            let handle =
                scope.spawn(|| cli.run_cancellable(&["gs2-helper"], &cancel, &options(65536)));
            let parent = wait_file(&fixture, "parent.pid");
            let child = wait_file(&fixture, "child.pid");
            let started = Instant::now();
            let pause = navo_git::cli::stop_under(fixture.repo(), Duration::from_secs(2)).unwrap();
            assert!(
                started.elapsed() < Duration::from_millis(300),
                "the stop took {:?}",
                started.elapsed()
            );
            let result = handle.join().unwrap();
            drop(pause);
            assert!(matches!(result, Err(GitError::Paused)));
            assert_reaped(&parent);
            assert!(!running(&child));
        });
    }

    #[cfg(unix)]
    #[test]
    fn stdout_cap_preserves_exact_boundary_and_kills_writer() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "echo $$ > parent.pid\nhead -c 65537 /dev/zero\nsleep 20",
        );
        let output = cli
            .run_cancellable(&["gs2-helper"], &Cancel::default(), &options(65536))
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Capped);
        assert!(output.capped());
        assert_eq!(output.stdout, vec![0; 65536]);
        assert_reaped(&wait_file(&fixture, "parent.pid"));
    }

    #[cfg(windows)]
    fn native_sleeper(fixture: &Fixture) -> CliGit {
        cli(
            fixture,
            "powershell.exe -NoProfile -NonInteractive -Command '$PID | Set-Content -Encoding ascii child.pid; Start-Sleep 20'",
        )
    }

    #[cfg(windows)]
    fn native_running(pid: &str) -> bool {
        let output = Command::new("tasklist.exe")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(pid))
    }

    #[cfg(windows)]
    #[test]
    fn cancel_kills_native_descendant_within_100ms() {
        let fixture = fixture();
        let cli = native_sleeper(&fixture);
        let cancel = Cancel::default();
        thread::scope(|scope| {
            let handle = scope.spawn(|| cli.run_cancellable(&["gs2-helper"], &cancel, &patient()));
            let pid = wait_file(&fixture, "child.pid");
            let started = Instant::now();
            cancel.cancel();
            assert_eq!(
                handle.join().unwrap().unwrap().outcome,
                ProcessOutcome::Cancelled
            );
            assert!(started.elapsed() < Duration::from_millis(100));
            assert!(!native_running(&pid));
        });
    }

    /// Git for Windows' `sh` starts a background child that inherits the output pipes; the run
    /// ends only once that child is gone too.
    #[cfg(windows)]
    #[test]
    fn cancel_kills_an_msys_background_child_that_holds_the_pipes() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "sleep 60 &\ncat /proc/$!/winpid > child.pid\nwait",
        );
        let cancel = Cancel::default();
        thread::scope(|scope| {
            let handle = scope.spawn(|| cli.run_cancellable(&["gs2-helper"], &cancel, &patient()));
            let pid = wait_file(&fixture, "child.pid");
            let started = Instant::now();
            cancel.cancel();
            assert_eq!(
                handle.join().unwrap().unwrap().outcome,
                ProcessOutcome::Cancelled
            );
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the run waited for the background sleep: {:?}",
                started.elapsed()
            );
            assert!(!native_running(&pid));
        });
    }

    /// A registry stop pauses a cancellable run and kills its MSYS child.
    #[cfg(windows)]
    #[test]
    fn stop_under_pauses_a_running_cancellable_call_on_windows() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "sleep 60 &\ncat /proc/$!/winpid > child.pid\nwait",
        );
        let cancel = Cancel::default();
        thread::scope(|scope| {
            let handle =
                scope.spawn(|| cli.run_cancellable(&["gs2-helper"], &cancel, &options(65536)));
            let child = wait_file(&fixture, "child.pid");
            let started = Instant::now();
            let pause = navo_git::cli::stop_under(fixture.repo(), Duration::from_secs(2)).unwrap();
            assert!(
                started.elapsed() < Duration::from_millis(300),
                "the stop took {:?}",
                started.elapsed()
            );
            let result = handle.join().unwrap();
            drop(pause);
            assert!(matches!(result, Err(GitError::Paused)));
            assert!(!native_running(&child));
        });
    }

    /// A normal exit still terminates descendants left in the Windows job object.
    #[cfg(windows)]
    #[test]
    fn a_normal_exit_still_terminates_the_job() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "sleep 60 >/dev/null 2>&1 &\ncat /proc/$!/winpid > child.pid\nexit 0",
        );
        let output = cli
            .run_cancellable(&["gs2-helper"], &Cancel::default(), &patient())
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Done);
        let child = wait_file(&fixture, "child.pid");
        assert!(
            !native_running(&child),
            "a finished run must still kill its job"
        );
    }

    /// A deadline far past the test, so only the cancel can end the run.
    #[cfg(windows)]
    fn patient() -> ProcessOptions {
        ProcessOptions {
            timeout: Duration::from_secs(120),
            ..options(65536)
        }
    }

    #[cfg(windows)]
    #[test]
    fn timeout_kills_native_descendant() {
        let fixture = fixture();
        let cli = native_sleeper(&fixture);
        let output = cli
            .run_cancellable(
                &["gs2-helper"],
                &Cancel::default(),
                &ProcessOptions {
                    timeout: Duration::from_secs(2),
                    ..options(65536)
                },
            )
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::TimedOut);
        assert!(!native_running(&wait_file(&fixture, "child.pid")));
    }

    #[test]
    fn stdout_exact_limit_is_capped_even_without_an_extra_byte() {
        let fixture = fixture();
        let cli = cli(&fixture, "head -c 65536 /dev/zero");
        let output = cli
            .run_cancellable(&["gs2-helper"], &Cancel::default(), &options(65536))
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Capped);
        assert_eq!(output.stdout.len(), 65536);
    }

    #[test]
    fn stdout_below_limit_and_self_exit_preserve_status() {
        let fixture = fixture();
        let cli = cli(&fixture, "head -c 65535 /dev/zero\nexit 7");
        let output = cli
            .run_cancellable(&["gs2-helper"], &Cancel::default(), &options(65536))
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Done);
        assert_eq!(output.stdout.len(), 65535);
        assert_eq!(output.status.unwrap().code(), Some(7));
    }

    #[test]
    fn stderr_flood_is_drained_and_capped_without_deadlock() {
        let fixture = fixture();
        let cli = cli(&fixture, "head -c 1048576 /dev/zero >&2\nprintf payload");
        let output = cli
            .run_cancellable(
                &["gs2-helper"],
                &Cancel::default(),
                &ProcessOptions {
                    timeout: Duration::from_secs(60),
                    ..options(65536)
                },
            )
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Done);
        // The first and the last 8 KiB, joined by "\n...\n".
        assert_eq!(output.stderr.len(), 2 * 8192 + 5);
        assert!(output.stderr.windows(5).any(|window| window == b"\n...\n"));
        assert_eq!(output.stdout, b"payload");
    }

    #[test]
    fn cancel_before_start_spawns_nothing() {
        let fixture = fixture();
        let cli = cli(&fixture, "echo started > parent.pid");
        let cancel = Cancel::default();
        cancel.cancel();
        let output = cli
            .run_cancellable(&["gs2-helper"], &cancel, &options(65536))
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Cancelled);
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert!(output.status.is_none());
        assert!(!fixture.repo().join("parent.pid").exists());
    }

    #[test]
    fn zero_cap_stops_without_reading_bytes() {
        let fixture = fixture();
        let cli = cli(&fixture, "sleep 20");
        let output = cli
            .run_cancellable(&["gs2-helper"], &Cancel::default(), &options(0))
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::Capped);
        assert!(output.stdout.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn repeated_runs_leave_no_reader_or_watchdog_threads() {
        let fixture = fixture();
        let cli = cli(
            &fixture,
            "case \"$4\" in stall) echo ready > child.ready; sleep 20;; esac\nprintf data",
        );
        let before = thread_count();
        for cap in [4, 5] {
            let output = cli
                .run_cancellable(&["gs2-helper"], &Cancel::default(), &options(cap))
                .unwrap();
            assert_eq!(output.capped(), cap == 4);
            assert_eq!(
                settled_thread_count(before),
                before,
                "readers and watchdog must end on exit and cap"
            );
        }
        let output = cli
            .run_cancellable(
                &["gs2-helper", "stall"],
                &Cancel::default(),
                &ProcessOptions {
                    timeout: Duration::from_millis(50),
                    ..options(65536)
                },
            )
            .unwrap();
        assert_eq!(output.outcome, ProcessOutcome::TimedOut);
        assert_eq!(
            settled_thread_count(before),
            before,
            "readers and watchdog must end on timeout"
        );
        fs::remove_file(fixture.repo().join("child.ready")).unwrap();
        let cancel = Cancel::default();
        thread::scope(|scope| {
            let handle = scope
                .spawn(|| cli.run_cancellable(&["gs2-helper", "stall"], &cancel, &options(65536)));
            wait_file(&fixture, "child.ready");
            cancel.cancel();
            assert_eq!(
                handle.join().unwrap().unwrap().outcome,
                ProcessOutcome::Cancelled
            );
        });
        assert_eq!(
            settled_thread_count(before),
            before,
            "readers and watchdog must end on cancel"
        );
    }
}
