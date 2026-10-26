use super::report::{Report, guard, markdown, parse};

fn sample() -> serde_json::Value {
    let snapshot =
        include_str!("../../../crates/rusty-app/src/metrics/snapshots/metrics_report.snap");
    let json = snapshot.splitn(3, "---").nth(2).unwrap();
    serde_json::from_str(json).unwrap()
}

fn report() -> Report {
    parse(&sample().to_string()).unwrap()
}

fn warm(times: [f64; 5]) -> Vec<Report> {
    times
        .into_iter()
        .map(|first_frame_ms| {
            let mut report = report();
            report.first_frame_ms = first_frame_ms;
            report
        })
        .collect()
}

#[test]
fn parses_the_real_metrics_schema_and_reports_metadata() {
    let first = report();
    let warm = warm([120.0, 80.0, 160.0, 100.0, 140.0]);
    let table = markdown(&first, &warm, &guard(&first, &warm));
    for text in [
        "Microsoft Basic Render Driver",
        "Version: 0.4.0 · Commit: abc123def456",
        "Exe: 31457280 bytes (31.46 MB)",
        "| Start to first frame | first | 212.00 ms | 212.00 ms | 212.00 ms | ≤ 400 ms |",
        "| Start to first frame | warm | 80.00 ms | 120.00 ms | 160.00 ms | ≤ 150 ms |",
        "| First usable frame | warm | 260.00 ms | 260.00 ms | 260.00 ms | ≤ 150 ms |",
        "| Private bytes | warm | 52.43 MB | 52.43 MB | 52.43 MB | < 80 MB |",
        "| Working set | warm | 73.40 MB | 73.40 MB | 73.40 MB | < 80 MB |",
        "| Peak working set | warm | 80.00 MB | 80.00 MB | 80.00 MB | < 80 MB |",
        "Guard: passed",
    ] {
        assert!(table.contains(text), "missing {text}: {table}");
    }
}

#[test]
fn memory_statistics_use_all_five_warm_samples() {
    let mut warm = warm([100.0; 5]);
    for (report, bytes) in warm.iter_mut().zip([90, 50, 60, 80, 70]) {
        report.idle_private_bytes = Some(bytes * 1_000_000);
        report.idle_working_set_bytes = Some(bytes * 1_000_000);
        report.peak_working_set_bytes = Some(bytes * 1_000_000);
    }
    let first = report();
    let table = markdown(&first, &warm, &guard(&first, &warm));
    for metric in ["Private bytes", "Working set", "Peak working set"] {
        assert!(table.contains(&format!(
            "| {metric} | warm | 50.00 MB | 70.00 MB | 90.00 MB | < 80 MB |"
        )));
    }
}

#[test]
fn guard_accepts_exactly_twice_the_targets() {
    let mut first = report();
    first.idle_private_bytes = Some(160_000_000);
    assert!(guard(&first, &warm([300.0; 5])).is_ok());
}

#[test]
fn guard_rejects_median_warm_start_above_twice_target() {
    let first = report();
    let warm = warm([100.0, 300.1, 300.1, 300.1, 100.0]);
    assert!(guard(&first, &warm).unwrap_err().contains("300.1 ms"));
}

#[test]
fn guard_rejects_a_three_times_slowdown() {
    assert!(guard(&report(), &warm([450.0; 5])).is_err());
}

#[test]
fn guard_uses_the_median_not_a_warm_outlier() {
    assert!(guard(&report(), &warm([100.0, 100.0, 100.0, 900.0, 900.0])).is_ok());
}

#[test]
fn guard_rejects_private_bytes_in_the_first_or_any_warm_run() {
    for index in 0..6 {
        let mut first = report();
        let mut warm = warm([100.0; 5]);
        let report = if index == 0 {
            &mut first
        } else {
            &mut warm[index - 1]
        };
        report.idle_private_bytes = Some(160_000_001);
        let error = guard(&first, &warm).unwrap_err();
        assert!(error.contains("private bytes 160000001"));
        assert!(markdown(&first, &warm, &Err(error)).contains("Guard: FAILED"));
    }
}

#[test]
fn guard_only_checks_warm_start_and_private_bytes() {
    let mut first = report();
    first.first_frame_ms = 5_000.0;
    let mut warm = warm([200.0; 5]);
    for report in &mut warm {
        report.first_usable_ms = 5_000.0;
        report.idle_working_set_bytes = Some(500_000_000);
        report.peak_working_set_bytes = Some(600_000_000);
    }
    assert!(guard(&first, &warm).is_ok());
}

#[test]
fn rejects_invalid_measurements_and_missing_frame_times() {
    for key in [
        "first_frame_ms",
        "first_usable_ms",
        "idle_private_bytes",
        "idle_working_set_bytes",
        "peak_working_set_bytes",
        "exe_bytes",
    ] {
        let mut json = sample();
        if key.ends_with("_ms") {
            json.as_object_mut().unwrap().remove(key);
            assert!(parse(&json.to_string()).is_err(), "missing {key}");
            json[key] = serde_json::Value::Null;
            assert!(parse(&json.to_string()).is_err(), "null {key}");
        }
        json[key] = serde_json::json!(-1);
        assert!(parse(&json.to_string()).is_err(), "negative {key}");
    }
    assert!(parse("not JSON").is_err());
}

#[test]
fn unavailable_memory_and_exe_size_are_reported_without_a_regression() {
    let mut json = sample();
    for key in [
        "exe_bytes",
        "idle_private_bytes",
        "idle_working_set_bytes",
        "peak_working_set_bytes",
    ] {
        json[key] = serde_json::Value::Null;
    }
    let first = parse(&json.to_string()).unwrap();
    let warm = vec![first.clone(); 5];
    let guard = guard(&first, &warm);
    assert!(guard.is_ok());
    let markdown = markdown(&first, &warm, &guard);
    assert!(markdown.contains("Exe: N/A"));
    assert!(markdown.contains("| Private bytes | warm | N/A | N/A | N/A | < 80 MB |"));
}

#[test]
fn markdown_escapes_renderer_metadata() {
    let mut json = sample();
    json["renderer"] = serde_json::json!("<gpu>|\nname");
    let first = parse(&json.to_string()).unwrap();
    let warm = warm([100.0; 5]);
    assert!(markdown(&first, &warm, &Ok(())).contains("&lt;gpu&gt;&#124; name"));
}

struct TestRoot(std::path::PathBuf);

impl TestRoot {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("navo-perf-{}-{timestamp}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write_reports(&self, milliseconds: f64) {
        let mut json = sample();
        json["first_frame_ms"] = serde_json::json!(milliseconds);
        for name in ["first", "warm-1", "warm-2", "warm-3", "warm-4", "warm-5"] {
            std::fs::write(self.0.join(format!("{name}.json")), json.to_string()).unwrap();
        }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn summary_is_appended_before_a_regression_returns_failure() {
    let root = TestRoot::new();
    root.write_reports(450.0);
    let output = root.0.join("summary.md");
    std::fs::write(&output, "Existing summary\n").unwrap();
    assert!(super::summarize(&root.0, &output).is_err());
    let markdown = std::fs::read_to_string(&output).unwrap();
    assert!(markdown.starts_with("Existing summary\n"));
    assert!(markdown.contains("Guard: FAILED: median warm start 450.0 ms > 300 ms"));
}

#[test]
fn summary_requires_all_six_reports() {
    let root = TestRoot::new();
    root.write_reports(100.0);
    std::fs::remove_file(root.0.join("warm-5.json")).unwrap();
    let error = super::summarize(&root.0, &root.0.join("summary.md")).unwrap_err();
    assert!(error.contains("warm-5.json"));
}

#[test]
fn fixture_has_fixed_names_and_counts_on_repeated_generation() {
    let root = TestRoot::new();
    for _ in 0..2 {
        super::fixture::generate(&root.0).unwrap();
        let left = root.0.join("A");
        let right = root.0.join("B");
        assert_eq!(std::fs::read_dir(&left).unwrap().count(), 5_000);
        assert_eq!(std::fs::read_dir(&right).unwrap().count(), 200);
        assert!(left.join("file-0000.txt").is_file());
        assert!(left.join("file-4999.txt").is_file());
        for index in 0..200 {
            let folder = right.join(format!("folder-{index:03}"));
            assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 25);
            assert!(folder.join("file-0000.txt").is_file());
            assert!(folder.join("file-0024.txt").is_file());
        }
    }
}
