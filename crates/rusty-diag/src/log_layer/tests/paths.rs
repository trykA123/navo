use serde_json::json;

use super::{TARGET, capture, is_redacted_path, redacted, redactor};
use crate::{DriveKind, PathKind};

const RAW: &str = r"C:\Users\canary-user-7731\Documents\plan.txt";

#[test]
fn raw_str_path_field_is_dropped() {
    let c = capture(|| tracing::info!(target: TARGET, path = RAW, "open"));
    assert!(!c.only_fields().contains_key("path"), "{}", c.only_raw());
}

#[test]
fn raw_str_path_field_counts_one_miss() {
    let c = capture(|| tracing::info!(target: TARGET, path = RAW, "open"));
    assert_eq!(c.misses.count(), 1);
}

#[test]
fn other_fields_stay_when_a_path_field_is_dropped() {
    let c = capture(|| tracing::info!(target: TARGET, tab = 2, path = RAW, ok = true, "open"));
    let f = c.only_fields();
    assert_eq!(
        (f.get("tab"), f.get("ok")),
        (Some(&json!(2)), Some(&json!(true)))
    );
}

#[test]
fn raw_str_suffix_path_field_is_dropped_as_a_miss() {
    let c = capture(|| tracing::info!(target: TARGET, dest_path = RAW, "copy"));
    let dropped = !c.only_fields().contains_key("dest_path");
    assert!(dropped && c.misses.count() == 1, "{}", c.only_raw());
}

#[test]
fn raw_str_path_field_is_a_miss_even_when_already_redacted() {
    let r = redactor();
    let p = redacted(&r, RAW);
    let c = capture(|| tracing::info!(target: TARGET, path = p.as_str(), "open"));
    let dropped = !c.only_fields().contains_key("path");
    assert!(dropped && c.misses.count() == 1, "{}", c.only_raw());
}

#[test]
fn display_of_a_redacted_path_is_kept() {
    let r = redactor();
    let p = redacted(&r, RAW);
    let c = capture(|| tracing::info!(target: TARGET, path = %p, "open"));
    assert_eq!(c.only_fields().get("path"), Some(&json!(p.as_str())));
}

#[test]
fn display_of_a_redacted_path_is_not_a_miss() {
    let r = redactor();
    let p = redacted(&r, RAW);
    let c = capture(|| tracing::info!(target: TARGET, path = %p, "open"));
    assert_eq!(c.misses.count(), 0);
}

#[test]
fn display_of_a_redacted_suffix_path_is_kept() {
    let r = redactor();
    let p = redacted(&r, r"\\canary-srv-7731\share\q3.xlsx");
    let c = capture(|| tracing::info!(target: TARGET, source_path = %p, "copy"));
    assert_eq!(c.only_fields().get("source_path"), Some(&json!(p.as_str())));
}

#[test]
fn display_of_a_raw_path_string_is_dropped_as_a_miss() {
    let c = capture(|| tracing::info!(target: TARGET, path = %"C:\\x", "open"));
    let dropped = !c.only_fields().contains_key("path");
    assert!(dropped && c.misses.count() == 1, "{}", c.only_raw());
}

#[test]
fn misses_add_up_across_events() {
    let c = capture(|| {
        tracing::info!(target: TARGET, path = RAW, "a");
        tracing::warn!(target: TARGET, old_path = RAW, new_path = RAW, "b");
    });
    assert_eq!(c.misses.count(), 3);
}

#[test]
fn field_merely_containing_path_is_not_a_path_field() {
    let c = capture(|| tracing::info!(target: TARGET, paths = 4, pathway = "x", "scan"));
    let f = c.only_fields();
    assert_eq!(
        (f.get("paths"), f.get("pathway")),
        (Some(&json!(4)), Some(&json!("x")))
    );
}

#[test]
fn is_redacted_path_accepts_redacted_shapes() {
    let accepted = [
        r"C:\<user>\<d:3fa2>\<f:07e1>.jpg",
        r"\\<server:1>\<share:1>\<d:0000>",
        r"\\<server:12>\<share:3>\<f:beef>.xlsx",
        r"D:\<d:91bc>\<f:07e1>",
        r"C:\<user>",
        "/<user>/<d:abcd>/<f:0123>.md",
        "/<d:abcd>/<f:0123>.tar9",
        r"<user>\<f:07e1>.abcdefgh",
        r"<d:3fa2>\<f:07e1>.jpg",
    ];
    let refused: Vec<&str> = accepted
        .into_iter()
        .filter(|t| !is_redacted_path(t))
        .collect();
    assert!(refused.is_empty(), "refused: {refused:#?}");
}

#[test]
fn is_redacted_path_rejects_everything_else() {
    let rejected = [
        "",
        r"C:\Users",
        r"C:\<user>\Documents",
        r"C:\<d:xyz1>\<f:07e1>.jpg",
        r"C:\<d:3fa>\<f:07e1>.jpg",
        r"C:\<d:3fa2>\<f:07e1>.JPG",
        r"C:\<d:3fa2>\<f:07e1>.abcdefghi",
        r"C:\<d:3fa2>.jpg",
        r"C:\<f:07e1>.jpg extra",
        r"\\server\<share:1>\<d:0000>",
        r"\\<server:x>\<share:1>\<d:0000>",
        "<computer>",
        "/home/admin",
    ];
    let accepted: Vec<&str> = rejected
        .into_iter()
        .filter(|t| is_redacted_path(t))
        .collect();
    assert!(accepted.is_empty(), "accepted: {accepted:#?}");
}

#[test]
fn is_redacted_path_accepts_every_redactor_output() {
    let r = redactor();
    let inputs = [
        (RAW, PathKind::File, DriveKind::Local),
        (
            r"C:\Users\canary-user-7731",
            PathKind::Folder,
            DriveKind::Local,
        ),
        (
            r"D:\Projects\rusty\Cargo.toml",
            PathKind::File,
            DriveKind::Removable,
        ),
        (
            r"\\canary-srv-7731\share\Reports\2026",
            PathKind::Folder,
            DriveKind::Network,
        ),
        (r"\\?\C:\Data\archive.bin", PathKind::File, DriveKind::Local),
        ("/srv/media/film.mkv", PathKind::File, DriveKind::Unknown),
    ];
    let refused: Vec<String> = inputs
        .iter()
        .map(|(p, kind, drive)| r.path(p, *kind, *drive).as_str().to_owned())
        .filter(|t| !is_redacted_path(t))
        .collect();
    assert!(refused.is_empty(), "refused: {refused:#?}");
}
