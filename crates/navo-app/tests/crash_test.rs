use std::fs;
use std::process::Command;

#[test]
fn crash_test_refuses_to_write_without_the_exact_guard() {
    let root = tempfile::tempdir().unwrap();
    for guard in [None, Some("0"), Some("true")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_navo"));
        command.arg("--crash-test").arg(root.path());
        command.env_remove("NAVO_CRASH_TEST");
        if let Some(value) = guard {
            command.env("NAVO_CRASH_TEST", value);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn guarded_crash_test_writes_a_record_in_the_given_profile() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_navo"))
        .arg("--crash-test")
        .arg(root.path())
        .env("NAVO_CRASH_TEST", "1")
        .env("NAVO_LOG", "info")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101), "{output:?}");
    let log = fs::read_to_string(root.path().join("data/logs/navo.log")).unwrap();
    assert!(
        log.contains("crash test about to panic"),
        "the panic hook must flush buffered logs"
    );
    let records: Vec<_> = fs::read_dir(root.path().join("data/logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("crash-")
        })
        .collect();
    assert_eq!(records.len(), 1);
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(&records[0]).unwrap()).unwrap();
    assert_eq!(record["message"], "Navo CI crash test");
    let backtrace = record["backtrace"].as_str().unwrap();
    let mut lines = backtrace.lines();
    // Line-tables-only keeps just the bare symbol; full debug info adds the crate path.
    assert!(
        lines.any(|line| line.contains("navo::crash_test") || line.trim().ends_with("crash_test")),
        "{backtrace}"
    );
    let location = lines.next().unwrap_or_default();
    assert!(
        location.trim_start().starts_with("at ") && location.contains("main.rs:"),
        "the crash_test frame has no source location: {backtrace}"
    );
}

#[cfg(windows)]
#[allow(clippy::unwrap_used, reason = "test helpers fail loudly")]
mod native {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus};
    use std::time::{Duration, Instant};

    use serde_json::Value;

    /// Waits for one native crash record and checks that the probe left nothing unredacted.
    #[test]
    fn native_crash_test_writes_a_redacted_record() {
        let root = temp_profile();
        let logs = root.path().join("data/logs");
        let (mut child, socket) = spawn_probe(root.path(), "native");

        let records = wait_for(Duration::from_secs(15), || {
            let records = crash_files(&logs, "json");
            (!records.is_empty()).then_some(records)
        });
        let status = wait_child(&mut child, Duration::from_secs(15));

        let Some(records) = records else {
            stop(&mut child);
            panic!("no crash-*.json appeared in {}", logs.display());
        };
        let Some(status) = status else {
            stop(&mut child);
            panic!("the native crash probe did not exit");
        };
        assert_eq!(records.len(), 1, "one crash gives one record: {records:?}");
        assert!(
            !matches!(status.code(), Some(0 | 2 | 3 | 101)),
            "the native crash probe exited like a normal, guard or panic path: {status:?}"
        );

        let record: Value = serde_json::from_slice(&fs::read(&records[0]).unwrap()).unwrap();
        assert_native_record(&record);
        assert_redacted(&record);
        assert_eq!(
            crash_files(&logs, "dmp").len(),
            1,
            "one crash gives one dump"
        );
        assert!(
            wait_for(Duration::from_secs(5), || (!socket.exists()).then_some(())).is_some(),
            "the crash server socket survived: {}",
            socket.display()
        );
    }

    /// Runs the panic probe and checks that its crash server socket is gone afterwards.
    #[test]
    fn panic_crash_leaves_no_crash_server() {
        let root = temp_profile();
        let (mut child, socket) = spawn_probe(root.path(), "1");

        let status = wait_child(&mut child, Duration::from_secs(15));
        let Some(status) = status else {
            stop(&mut child);
            panic!("the panic crash probe did not exit");
        };
        assert_eq!(status.code(), Some(101), "{status:?}");
        assert!(
            wait_for(Duration::from_secs(5), || (!socket.exists()).then_some(())).is_some(),
            "the crash server socket survived the panic: {}",
            socket.display()
        );
    }

    /// Creates a temporary profile whose path contains a space, so the server args need quoting.
    fn temp_profile() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("navo crash ")
            .tempdir()
            .unwrap()
    }

    /// Spawns `navo --crash-test root` with `guard` and returns the child and its socket path.
    fn spawn_probe(root: &Path, guard: &str) -> (Child, PathBuf) {
        let child = Command::new(env!("CARGO_BIN_EXE_navo"))
            .arg("--crash-test")
            .arg(root)
            .env("NAVO_CRASH_TEST", guard)
            .env("NAVO_LOG", "info")
            .spawn()
            .unwrap();
        let socket = std::env::temp_dir().join(format!("navo-crash-{}.sock", child.id()));
        (child, socket)
    }

    /// Checks the message, location and first backtrace line of a native crash record.
    fn assert_native_record(record: &Value) {
        let message = record["message"].as_str().unwrap();
        assert!(message.starts_with("native crash: "), "{message}");
        let reason = message.to_ascii_lowercase().replace('_', " ");
        assert!(reason.contains("access violation"), "{message}");
        let location = record["location"].as_str().unwrap();
        assert!(location.starts_with("navo.exe+0x"), "{location}");
        let backtrace = record["backtrace"].as_str().unwrap();
        assert_eq!(
            backtrace.lines().next(),
            Some("stack scan (may include stale frames)")
        );
    }

    /// Polls `probe` until it returns a value or `timeout` passes.
    fn wait_for<T>(timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(value) = probe() {
                return Some(value);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits up to `timeout` for the child to exit, leaving it running on timeout.
    fn wait_child(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
        wait_for(timeout, || child.try_wait().unwrap())
    }

    /// Kills and reaps the child unless it already exited.
    fn stop(child: &mut Child) {
        if child.try_wait().unwrap().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Returns the `crash-*` files in `logs` with `extension`; a missing folder gives none.
    fn crash_files(logs: &Path, extension: &str) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(logs) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().and_then(|name| name.to_str());
                name.is_some_and(|name| name.starts_with("crash-"))
                    && path.extension().and_then(|ext| ext.to_str()) == Some(extension)
            })
            .collect()
    }

    /// Fails when any record string holds a drive or UNC path, the user name or the profile path.
    fn assert_redacted(record: &Value) {
        let mut strings = Vec::new();
        collect_strings(record, &mut strings);
        let username = std::env::var("USERNAME").unwrap();
        let profile = std::env::var("USERPROFILE").unwrap();
        assert!(
            !username.is_empty(),
            "USERNAME must be set for the redaction check"
        );
        assert!(
            !profile.is_empty(),
            "USERPROFILE must be set for the redaction check"
        );
        let username = if username.len() >= 3 {
            Some(username.to_ascii_lowercase())
        } else {
            None
        };
        let profile = profile.to_ascii_lowercase();
        for text in strings {
            assert!(
                !text.contains(":\\") && !text.contains(":/"),
                "a drive path leaked: {text}"
            );
            assert!(!text.contains(r"\\"), "a UNC path leaked: {text}");
            let lower = text.to_ascii_lowercase();
            if let Some(username) = username.as_deref() {
                assert!(!lower.contains(username), "the user name leaked: {text}");
            }
            assert!(!lower.contains(&profile), "the profile path leaked: {text}");
        }
    }

    /// Appends every string in `value`, including nested arrays and objects.
    fn collect_strings(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(text) => out.push(text.clone()),
            Value::Array(items) => {
                for item in items {
                    collect_strings(item, out);
                }
            }
            Value::Object(fields) => {
                for value in fields.values() {
                    collect_strings(value, out);
                }
            }
            _ => {}
        }
    }
}
