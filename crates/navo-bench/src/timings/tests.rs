use serde_json::Value;

use super::bench_json::parse_bench;
use super::error::TimingError;
use super::fake::{Bench, Fake, SECRET};
use super::keys::keys;
use super::run::{GENERATED_FILES, Plan, measure_folder_timings};
use super::trace::parse_trace;
use crate::fixture;
use crate::harness::phase::{Observer, Verdict};
use crate::privacy::{LeakScanner, Output, Report, SafeText, SafeWriter, assert_no_leaks};

const BOTH: Plan = Plan {
    owner: true,
    generated: true,
};
const QUICK: Plan = Plan {
    owner: true,
    generated: false,
};

#[derive(Default)]
struct Seen {
    started: Vec<String>,
    verdicts: Vec<Verdict>,
}

impl Observer for Seen {
    fn started(&mut self, name: SafeText) {
        let text = serde_json::to_value(name).unwrap_or_default();
        self.started
            .push(text.as_str().unwrap_or_default().to_owned());
    }
    fn finished(&mut self, _name: SafeText, verdict: Verdict) {
        self.verdicts.push(verdict);
    }
}

fn run(fake: &mut Fake, plan: Plan) -> (Vec<Value>, Seen) {
    let mut seen = Seen::default();
    let reports = measure_folder_timings(fake, plan, &mut seen);
    let values = reports
        .iter()
        .map(|report| serde_json::to_value(report).unwrap())
        .collect();
    (values, seen)
}

fn find<'a>(reports: &'a [Value], measure: &str, subject: &str) -> &'a Value {
    reports
        .iter()
        .find(|r| r["measure"] == measure && r["subject"] == subject)
        .unwrap()
}

fn reason(metric: &Value) -> &str {
    assert_eq!(metric["state"], "not-measured");
    metric["reason"].as_str().unwrap()
}

#[test]
fn phases_run_in_order_and_quick_skips_the_generated_folder() {
    let (_, seen) = run(&mut Fake::new(), BOTH);
    assert_eq!(seen.started.len(), 8);
    assert_eq!(seen.started[1], "100k folder: build");
    assert_eq!(seen.started[2], "owner folder: list, sort, filter");
    assert_eq!(seen.started[7], "100k folder: key to frame");
    let mut fake = Fake::new();
    let (reports, seen) = run(&mut fake, QUICK);
    assert_eq!(seen.started.len(), 4);
    assert!(reports.iter().all(|r| r["subject"] != "generated"));
    assert!(fake.events().iter().all(|e| !e.contains("generate")));
}

#[test]
fn nothing_runs_without_a_folder_to_time() {
    let none = Plan {
        owner: false,
        generated: false,
    };
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake, none);
    assert!(reports.is_empty());
    assert_eq!(fake.events(), ["prepare"]);
}

#[test]
fn the_generated_folder_is_built_once_and_its_time_is_reported() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake, BOTH);
    let built = fake
        .events()
        .iter()
        .filter(|e| e.starts_with("generate"))
        .count();
    assert_eq!(built, 1);
    assert!(
        fake.events()
            .contains(&format!("generate {GENERATED_FILES}"))
    );
    let generated = &reports[0];
    assert_eq!(generated["measure"], "generated");
    assert_eq!(generated["files"], 100_000);
    assert_eq!(generated["build_micros"]["value"], 2_500_000);
}

#[test]
fn the_headless_benchmark_runs_five_times_and_pools_the_numbers() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake, QUICK);
    let owner = find(&reports, "headless", "owner");
    assert_eq!(
        (owner["attempted"].as_u64(), owner["failed"].as_u64()),
        (Some(5), Some(0))
    );
    assert_eq!(owner["entries"]["value"], 100_000);
    assert_eq!(owner["top_level_bytes"]["value"], 4096);
    assert_eq!(owner["list"]["value"]["samples"], 25);
    assert_eq!(owner["filter_keystroke"]["value"]["samples"], 75);
    let keys: Vec<_> = owner["sorts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["key"].clone())
        .collect();
    assert_eq!(keys, ["size", "date", "extension", "name"]);
    for sort in owner["sorts"].as_array().unwrap() {
        assert_eq!(sort["time"]["value"]["samples"], 25);
    }
    assert_eq!(owner["sorts"][0]["time"]["value"]["median_micros"], 3_000);
    assert_eq!(owner["list"]["value"]["p95_is_max"], false);
}

#[test]
fn first_usable_comes_from_five_launches_with_the_folder() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake, QUICK);
    let usable = find(&reports, "first-usable", "owner");
    assert_eq!(usable["first_usable"]["value"]["samples"], 5);
    assert_eq!(usable["first_usable"]["value"]["median_micros"], 208_000);
    let launches = fake
        .events()
        .iter()
        .filter(|e| e.contains("metrics=true trace=false"))
        .count();
    assert_eq!(launches, 5);
}

#[test]
fn plain_letters_are_posted_one_by_one_and_key_to_frame_is_read_from_the_trace() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake, QUICK);
    let frames = find(&reports, "key-to-frame", "owner");
    assert_eq!(frames["keys_posted"], 20);
    assert_eq!(frames["records"], 20);
    assert_eq!(frames["key_to_frame"]["value"]["samples"], 20);
    assert_eq!(frames["key_to_frame"]["value"]["p95_is_max"], false);
    let posts: Vec<_> = fake
        .events()
        .into_iter()
        .filter(|e| e.starts_with("post"))
        .collect();
    assert_eq!(posts.len(), 20);
    let one = format!("{:?}", &keys()[..1]);
    assert!(posts[0].ends_with(&one), "{} {one}", posts[0]);
}

#[test]
fn a_build_without_trace_output_is_not_measured_with_a_reason() {
    let mut fake = Fake::new();
    fake.trace_records = None;
    let (reports, seen) = run(&mut fake, QUICK);
    let frames = find(&reports, "key-to-frame", "owner");
    assert_eq!(
        reason(&frames["key_to_frame"]),
        "build wrote no trace records"
    );
    assert_eq!(frames["failed"], 5);
    let code = TimingError::TraceMissing.code();
    assert!(matches!(seen.verdicts[3], Verdict::Failed(c) if c == code));
    assert!(matches!(seen.verdicts[1], Verdict::Ok));
}

#[test]
fn a_trace_that_holds_no_records_for_the_posted_keys_is_not_measured() {
    let mut fake = Fake::new();
    fake.trace_records = Some(0);
    let (reports, _) = run(&mut fake, QUICK);
    let frames = find(&reports, "key-to-frame", "owner");
    assert_eq!(
        (frames["keys_posted"].as_u64(), frames["records"].as_u64()),
        (Some(20), Some(0))
    );
    assert_eq!(
        reason(&frames["key_to_frame"]),
        "build traced no key events"
    );
}

#[test]
fn a_build_without_a_folder_benchmark_is_not_measured_and_the_rest_goes_on() {
    let mut fake = Fake::new();
    fake.bench = Bench::ExitsNonZero;
    let (reports, seen) = run(&mut fake, QUICK);
    let headless = find(&reports, "headless", "owner");
    assert_eq!(reason(&headless["list"]), "folder benchmark failed");
    assert_eq!(reason(&headless["entries"]), "folder benchmark failed");
    assert_eq!(headless["failed"], 5);
    assert!(matches!(seen.verdicts[1], Verdict::Failed(5)));
    let usable = find(&reports, "first-usable", "owner");
    assert_eq!(usable["first_usable"]["state"], "measured");
}

#[test]
fn unreadable_benchmark_output_is_not_measured() {
    let mut fake = Fake::new();
    fake.bench = Bench::Garbage;
    let (reports, _) = run(&mut fake, QUICK);
    let headless = find(&reports, "headless", "owner");
    assert_eq!(
        reason(&headless["list"]),
        "folder benchmark output unreadable"
    );
}

#[test]
fn a_failed_build_of_the_generated_folder_leaves_the_owner_numbers_alone() {
    let mut fake = Fake::new();
    fake.fail_generate = true;
    let (reports, seen) = run(&mut fake, BOTH);
    let generated = &reports[0];
    assert_eq!(
        reason(&generated["build_micros"]),
        "scratch folder not writable"
    );
    let headless = find(&reports, "headless", "generated");
    assert_eq!(reason(&headless["list"]), "100k folder not built");
    assert_eq!(
        reason(&find(&reports, "first-usable", "generated")["first_usable"]),
        "100k folder not built"
    );
    assert_eq!(
        find(&reports, "headless", "owner")["list"]["state"],
        "measured"
    );
    assert!(
        fake.events()
            .iter()
            .all(|e| !e.starts_with("bench generated"))
    );
    assert!(matches!(seen.verdicts[1], Verdict::Failed(_)));
}

#[test]
fn when_the_working_folders_fail_nothing_launches() {
    let mut fake = Fake::new();
    fake.fail_prepare = true;
    let (reports, _) = run(&mut fake, BOTH);
    let keys = find(&reports, "key-to-frame", "owner");
    assert_eq!(reason(&keys["key_to_frame"]), "working folders not built");
    assert!(fake.events().iter().all(|e| e == "prepare"));
}

#[test]
fn failed_launches_are_counted_and_the_other_runs_still_count() {
    let mut fake = Fake::new();
    fake.fail_launch = vec![7];
    let (reports, _) = run(&mut fake, QUICK);
    let usable = find(&reports, "first-usable", "owner");
    assert_eq!(usable["failed"], 1);
    assert_eq!(usable["first_usable"]["value"]["samples"], 4);
}

#[test]
fn a_launch_without_a_metrics_file_is_not_measured() {
    let mut fake = Fake::new();
    fake.no_metrics = true;
    let (reports, _) = run(&mut fake, QUICK);
    let usable = find(&reports, "first-usable", "owner");
    assert_eq!(
        reason(&usable["first_usable"]),
        "build wrote no metrics file"
    );
}

#[test]
fn bench_parsing_keeps_numbers_and_drops_labels_and_hostile_values() {
    let text = r#"{"entries":3,"bytes":"x","label":"zz","runs":[
        {"list_ms":-1.0,"sort_ms":[["size",1.0],["size",2.0],["bogus",5.0],["date","x"],7],"filter_keystroke_ms":[1,"a",null,1e12]},
        {"list_ms":4.5,"sort_ms":"no","filter_keystroke_ms":3},
        5, null]}"#;
    let numbers = parse_bench(text).unwrap();
    assert_eq!(numbers.entries, Some(3));
    assert_eq!(numbers.bytes, None);
    assert_eq!(numbers.list_ms, [4.5]);
    assert_eq!(numbers.sort_ms[0], [1.0]);
    assert!(numbers.sort_ms[1].is_empty());
    assert_eq!(numbers.filter_keystroke_ms, [1.0]);
    for bad in [
        "",
        "[]",
        "{}",
        r#"{"runs":[]}"#,
        r#"{"runs":7}"#,
        "{\"runs\":[{\"list_ms\":",
    ] {
        assert_eq!(parse_bench(bad), Err(TimingError::BenchJson), "{bad}");
    }
}

#[test]
fn bench_parsing_caps_the_number_of_runs() {
    let one = r#"{"list_ms":1.0,"sort_ms":[],"filter_keystroke_ms":[]}"#;
    let text = format!(r#"{{"runs":[{}]}}"#, vec![one; 500].join(","));
    assert_eq!(parse_bench(&text).unwrap().list_ms.len(), 64);
}

#[test]
fn trace_parsing_keeps_valid_numbers_only() {
    let text = "{\"action\":\"key:x\",\"key_to_frame_ms\":3.5}\nnot json\n\
        {\"key_to_frame_ms\":-1}\n{\"key_to_frame_ms\":\"7\"}\n{\"key_to_frame_ms\":99999999}\n\
        {\"key_to_frame_ms\":0}\n{\"key_to_frame_ms\":4";
    assert_eq!(parse_trace(text).key_to_frame_ms, [3.5, 0.0]);
    assert!(parse_trace("").key_to_frame_ms.is_empty());
}

#[test]
fn nothing_planted_reaches_the_output() {
    let mut fake = Fake::new();
    let mut seen = Seen::default();
    let reports = measure_folder_timings(&mut fake, BOTH, &mut seen);
    let needles = [SECRET, fixture::PRIVATE_FOLDER, fixture::PRIVATE_FILE];
    let scanner = LeakScanner::new(&needles).unwrap();
    let mut sink = Vec::new();
    for report in &reports {
        SafeWriter::new(&mut sink, scanner.clone())
            .write(Output::Json, report)
            .unwrap();
    }
    assert!(sink.len() > 2_000);
    assert_no_leaks(&sink, &needles).unwrap();
    assert!(matches!(reports[0], Report::Timings(_)));
    let errors = [
        TimingError::BenchExit,
        TimingError::TraceMissing,
        TimingError::NoFolder,
        TimingError::Io(std::io::ErrorKind::NotFound),
        TimingError::Probe(crate::probe::ProbeError::BadReadOnly),
    ];
    for error in errors {
        assert_no_leaks(error.to_string().as_bytes(), &needles).unwrap();
    }
}
