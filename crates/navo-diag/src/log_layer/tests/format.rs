use tracing::Level;

use super::{TARGET, capture};

#[test]
fn simple_event_becomes_one_line_in_the_ring() {
    let c = capture(|| tracing::info!(target: TARGET, "started"));
    assert_eq!(c.raw_lines().len(), 1);
}

#[test]
fn line_is_a_single_line_of_json() {
    let c = capture(|| tracing::info!(target: TARGET, "started"));
    assert!(!c.only_raw().trim_end().contains('\n'));
}

#[test]
fn line_has_exactly_the_five_keys() {
    let c = capture(|| tracing::info!(target: TARGET, n = 1, "started"));
    let keys: Vec<String> = c.only().as_object().unwrap().keys().cloned().collect();
    let mut expected = vec!["fields", "level", "msg", "t", "target"];
    expected.sort_unstable();
    let mut keys = keys;
    keys.sort_unstable();
    assert_eq!(keys, expected);
}

#[test]
fn keys_appear_in_the_documented_order() {
    let c = capture(|| tracing::info!(target: TARGET, n = 1, "started"));
    let raw = c.only_raw();
    let at = |key: &str| raw.find(&format!("\"{key}\":")).unwrap();
    assert!(raw.starts_with("{\"t\":"), "{raw}");
    assert!(at("t") < at("level") && at("level") < at("target"), "{raw}");
    assert!(
        at("target") < at("msg") && at("msg") < at("fields"),
        "{raw}"
    );
}

#[test]
fn t_is_whole_milliseconds() {
    let c = capture(|| tracing::info!(target: TARGET, "started"));
    assert!(c.only()["t"].is_u64(), "{}", c.only_raw());
}

#[test]
fn level_is_the_upper_case_name() {
    let c = capture(|| {
        tracing::trace!(target: TARGET, "a");
        tracing::debug!(target: TARGET, "b");
        tracing::info!(target: TARGET, "c");
        tracing::warn!(target: TARGET, "d");
        tracing::error!(target: TARGET, "e");
    });
    let levels: Vec<String> = c
        .lines()
        .iter()
        .map(|l| l["level"].as_str().unwrap().to_owned())
        .collect();
    let expected: Vec<String> = [
        Level::TRACE,
        Level::DEBUG,
        Level::INFO,
        Level::WARN,
        Level::ERROR,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(levels, expected);
}

#[test]
fn target_is_the_event_target() {
    let c = capture(|| tracing::info!(target: TARGET, "started"));
    assert_eq!(c.only()["target"], TARGET);
}

#[test]
fn msg_holds_the_formatted_message() {
    let c = capture(|| tracing::info!(target: TARGET, "loaded {} tabs", 3));
    assert_eq!(c.only()["msg"], "loaded 3 tabs");
}

#[test]
fn msg_is_redacted() {
    let c = capture(|| tracing::info!(target: TARGET, "signed in as canary-user-7731"));
    assert_eq!(c.only()["msg"], "signed in as <user>");
}

#[test]
fn message_is_not_repeated_in_fields() {
    let c = capture(|| tracing::info!(target: TARGET, n = 1, "started"));
    assert!(!c.only_fields().contains_key("message"), "{}", c.only_raw());
}

#[test]
fn fields_keep_the_order_they_were_recorded_in() {
    let c = capture(|| tracing::info!(target: TARGET, zeta = 1, alpha = 2, mid = 3, "x"));
    let raw = c.only_raw();
    let at = |key: &str| raw.find(&format!("\"{key}\":")).unwrap();
    assert!(at("zeta") < at("alpha") && at("alpha") < at("mid"), "{raw}");
}
