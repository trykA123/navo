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

#[test]
fn query_field_is_replaced_by_its_length() {
    let c = capture(|| tracing::info!(target: TARGET, query = "canary secret plan", "x"));
    assert_eq!(c.only_fields()["query"], json!("<text:18 chars>"));
}

#[test]
fn search_like_fields_are_replaced_by_their_length() {
    let c = capture(
        || tracing::info!(target: TARGET, search = "ab", filter = "été", pattern = "*.rs", name_query = "x", "x"),
    );
    let f = c.only_fields();
    let values: Vec<_> = ["search", "filter", "pattern", "name_query"]
        .iter()
        .map(|name| f[*name].clone())
        .collect();
    assert_eq!(
        values,
        [
            json!("<text:2 chars>"),
            json!("<text:3 chars>"),
            json!("<text:4 chars>"),
            json!("<text:1 chars>")
        ]
    );
}

#[test]
fn debug_query_field_counts_the_text_without_its_quotes() {
    let c = capture(|| tracing::info!(target: TARGET, query = ?"abc", "x"));
    assert_eq!(c.only_fields()["query"], json!("<text:3 chars>"));
}

#[test]
fn quoted_fragment_in_a_message_becomes_its_length() {
    let c = capture(|| tracing::warn!(target: TARGET, "cannot rename 'Secret Plan.docx'"));
    assert_eq!(c.only()["msg"], json!("cannot rename '<text:16 chars>'"));
}
