use super::metrics::{MetricsError, parse_metrics};
use crate::fixture;
use crate::privacy::assert_no_leaks;

/// The `--metrics` file of a tester build, with names planted in every string.
fn saved_sample() -> String {
    format!(
        r#"{{
  "version": "0.4.0",
  "commit": "abc123def456",
  "exe_bytes": 31457280,
  "renderer": "{folder}",
  "first_frame_ms": 212.0,
  "first_usable_ms": 260.0,
  "marks": [
    {{"name": "main", "ms": 8.0}},
    {{"name": "{query}", "ms": 11.0}},
    {{"name": "first frame", "ms": 212.0}},
    {{"name": "first usable", "ms": 260.0}}
  ],
  "idle_private_bytes": 52428800,
  "idle_working_set_bytes": 73400320,
  "peak_working_set_bytes": 80000000,
  "extra": "{file}"
}}"#,
        folder = fixture::PRIVATE_FOLDER,
        query = fixture::PRIVATE_QUERY,
        file = fixture::PRIVATE_FILE
    )
}

#[test]
fn the_saved_sample_gives_both_start_numbers() {
    let metrics = parse_metrics(&saved_sample()).unwrap();
    assert_eq!(metrics.first_frame_ms, Some(212.0));
    assert_eq!(metrics.first_usable_ms, Some(260.0));
}

#[test]
fn parsed_numbers_hold_none_of_the_strings_in_the_file() {
    let metrics = parse_metrics(&saved_sample()).unwrap();
    let shown = format!("{metrics:?}");
    let needles = [
        fixture::PRIVATE_FOLDER,
        fixture::PRIVATE_QUERY,
        fixture::PRIVATE_FILE,
    ];
    assert_no_leaks(shown.as_bytes(), &needles).unwrap();
    let failure = parse_metrics(
        &saved_sample()
            .replace("212.0", "\"x\"")
            .replace("260.0", "-1"),
    );
    assert_eq!(failure, Err(MetricsError::Empty));
    assert_no_leaks(failure.unwrap_err().to_string().as_bytes(), &needles).unwrap();
}

#[test]
fn marks_stand_in_when_the_fields_are_missing() {
    let json = r#"{"marks":[{"name":"first frame","ms":120.5},{"name":"first usable","ms":180}]}"#;
    let metrics = parse_metrics(json).unwrap();
    assert_eq!(metrics.first_frame_ms, Some(120.5));
    assert_eq!(metrics.first_usable_ms, Some(180.0));
}

#[test]
fn one_number_is_enough() {
    let metrics = parse_metrics(r#"{"first_usable_ms": 99.5}"#).unwrap();
    assert_eq!(metrics.first_frame_ms, None);
    assert_eq!(metrics.first_usable_ms, Some(99.5));
}

#[test]
fn bad_numbers_are_ignored_not_trusted() {
    let json = r#"{"first_frame_ms": -3, "first_usable_ms": 1e12,
        "marks": [{"name": "first frame", "ms": null}, 7, {"ms": 5}]}"#;
    assert_eq!(parse_metrics(json), Err(MetricsError::Empty));
}

#[test]
fn unreadable_text_is_a_kind_not_an_echo() {
    for text in ["", "not json", "[1,2]", r#"{"first_frame_ms":"#] {
        assert_eq!(parse_metrics(text), Err(MetricsError::Json), "{text:?}");
    }
    assert_eq!(parse_metrics("{}"), Err(MetricsError::Empty));
    assert_eq!(parse_metrics(r#"{"marks": 5}"#), Err(MetricsError::Empty));
}
