use std::fs;
use std::process::Command;

#[test]
fn crash_test_refuses_to_write_without_the_exact_guard() {
    let root = tempfile::tempdir().unwrap();
    for guard in [None, Some("0"), Some("true")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ferrite"));
        command.arg("--crash-test").arg(root.path());
        command.env_remove("FERRITE_CRASH_TEST");
        if let Some(value) = guard {
            command.env("FERRITE_CRASH_TEST", value);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn guarded_crash_test_writes_a_record_in_the_given_profile() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ferrite"))
        .arg("--crash-test")
        .arg(root.path())
        .env("FERRITE_CRASH_TEST", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101), "{output:?}");
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
    assert_eq!(record["message"], "Ferrite CI crash test");
    let backtrace = record["backtrace"].as_str().unwrap();
    assert!(backtrace.contains("ferrite::crash_test"), "{backtrace}");
}
