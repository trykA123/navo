use serde_json::json;

use super::{TARGET, capture};

#[test]
fn integers_stay_json_numbers() {
    let c = capture(|| tracing::info!(target: TARGET, count = 42_u64, delta = -7_i64, "x"));
    let f = c.only_fields();
    assert_eq!(
        (f["count"].clone(), f["delta"].clone()),
        (json!(42), json!(-7))
    );
}

#[test]
fn floats_stay_json_numbers() {
    let c = capture(|| tracing::info!(target: TARGET, ratio = 1.5_f64, "x"));
    assert_eq!(c.only_fields()["ratio"], json!(1.5));
}

#[test]
fn booleans_stay_json_booleans() {
    let c = capture(|| tracing::info!(target: TARGET, cached = true, stale = false, "x"));
    let f = c.only_fields();
    assert_eq!(
        (f["cached"].clone(), f["stale"].clone()),
        (json!(true), json!(false))
    );
}

#[test]
fn string_field_with_a_windows_path_is_redacted_through_text() {
    let raw = r"failed on C:\Users\canary-user-7731\Pictures\beach.jpg";
    let expected = super::redactor().text(raw);
    let c = capture(|| tracing::info!(target: TARGET, detail = raw, "x"));
    assert_eq!(c.only_fields()["detail"], json!(expected));
}

#[test]
fn display_field_is_redacted_through_text() {
    let c = capture(|| tracing::info!(target: TARGET, host = %"CANARY-PC-7731", "x"));
    assert_eq!(c.only_fields()["host"], json!("<computer>"));
}

#[test]
fn debug_field_is_redacted_through_text() {
    let owner = Some("canary-user-7731");
    let c = capture(|| tracing::info!(target: TARGET, owner = ?owner, "x"));
    assert_eq!(c.only_fields()["owner"], json!("Some(\"<user>\")"));
}
