#[cfg(unix)]
use std::fs;
use std::time::Duration;

use super::entry::Entry;
use super::mutate;
use super::*;
use crate::fixture::{self, Fixture};
use crate::privacy::assert_no_leaks;
use crate::source::{Record, WalkConfig, WalkOutcome, WalkSummary};

fn source(fx: &Fixture) -> ReadOnlySource {
    crate::source::test_open(fx.root()).unwrap()
}

fn capture(fx: &Fixture) -> Manifest {
    Manifest::capture(&source(fx))
}

/// Captures, applies `change`, captures again and returns the diff.
fn diff_after(fx: &Fixture, change: impl FnOnce() -> std::io::Result<()>) -> ManifestDiff {
    let before = capture(fx);
    change().unwrap();
    Manifest::diff(&before, &capture(fx))
}

fn only(diff: &ManifestDiff) -> [u64; 7] {
    [
        diff.added,
        diff.removed,
        diff.changed_size,
        diff.changed_mtime,
        diff.changed_attributes,
        diff.changed_content,
        diff.renamed_looking,
    ]
}

#[test]
fn unchanged_folder_is_equal() {
    let fx = fixture::set().unwrap();
    let (first, second) = (capture(&fx), capture(&fx));
    let diff = Manifest::diff(&first, &second);
    assert_eq!(diff.verdict, Verdict::Equal);
    assert_eq!(only(&diff), [0; 7]);
    assert_eq!(first.digest_hex(), second.digest_hex());
    assert!(first.is_complete());
    assert!(first.counts().files > 5);
    assert!(first.counts().hashed_bytes > 0);
}

#[test]
fn added_temp_file_is_one_addition() {
    let fx = fixture::set().unwrap();
    let before = capture(&fx);
    mutate::add_file(&fx.root().join("bench-temp.tmp")).unwrap();
    let after = capture(&fx);
    let diff = Manifest::diff(&before, &after);
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(only(&diff), [1, 0, 0, 0, 0, 0, 0]);
    assert_ne!(before.digest_hex(), after.digest_hex());
}

#[test]
fn removed_file_is_one_removal() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || mutate::remove_file(&fx.rare_file));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(only(&diff), [0, 1, 0, 0, 0, 0, 0]);
}

#[test]
fn renamed_file_is_renamed_looking_not_add_and_remove() {
    let fx = fixture::set().unwrap();
    let to = fx.root().join("renamed.txt");
    let diff = diff_after(&fx, || mutate::rename(&fx.privacy_file, &to));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(diff.renamed_looking, 1);
    assert_eq!((diff.added, diff.removed), (0, 0));
}

#[test]
fn renamed_folder_and_its_files_are_renamed_looking() {
    let fx = fixture::set().unwrap();
    let from = fx.privacy_file.parent().unwrap().to_path_buf();
    let diff = diff_after(&fx, || mutate::rename(&from, &fx.root().join("moved")));
    assert_eq!(diff.renamed_looking, 2);
    assert_eq!((diff.added, diff.removed), (0, 0));
}

#[test]
fn mtime_only_change_is_one_mtime_change() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || mutate::touch(&fx.rare_file));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(only(&diff), [0, 0, 0, 1, 0, 0, 0]);
}

#[test]
fn permission_only_change_is_one_attribute_change() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || mutate::toggle_readonly(&fx.rare_file));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(only(&diff), [0, 0, 0, 0, 1, 0, 0]);
}

#[test]
fn same_size_content_change_is_one_content_change() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || mutate::flip_byte_keep_mtime(&fx.privacy_file));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(only(&diff), [0, 0, 0, 0, 0, 1, 0]);
}

#[test]
fn appended_byte_is_a_size_change() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || mutate::append(&fx.privacy_file));
    assert_eq!(diff.changed_size, 1);
    assert_eq!(diff.changed_content, 1);
    assert_eq!((diff.added, diff.removed), (0, 0));
}

#[test]
fn added_hard_link_is_an_addition() {
    let fx = fixture::set().unwrap();
    let link = fx.root().join("extra-link.nbq");
    let diff = diff_after(&fx, || mutate::hard_link(&fx.rare_file, &link));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(diff.added, 1);
    assert_eq!((diff.removed, diff.changed_content), (0, 0));
    #[cfg(unix)]
    assert_eq!(diff.changed_attributes, 1);
}

#[cfg(unix)]
#[test]
fn retargeted_symlink_is_a_content_change() {
    let fx = fixture::set().unwrap();
    let link = fx.inner_link.clone().unwrap();
    let diff = diff_after(&fx, || mutate::retarget(&link, fixture::TARGET_B));
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!(diff.changed_content, 1);
    assert_eq!((diff.added, diff.removed, diff.changed_size), (0, 0, 0));
}

#[test]
fn file_replaced_by_folder_is_a_removal_and_an_addition() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || {
        mutate::remove_file(&fx.rare_file)?;
        mutate::make_dir(&fx.rare_file)
    });
    assert_eq!(diff.verdict, Verdict::Changed);
    assert_eq!((diff.added, diff.removed), (1, 1));
}

fn records_and_summary(source: &ReadOnlySource) -> (Vec<Record>, WalkSummary) {
    let mut records = Vec::new();
    let summary = source.walk(WalkConfig::default(), |record| records.push(record));
    (records, summary)
}

fn built(source: &ReadOnlySource, records: impl Iterator<Item = Record>) -> Manifest {
    let mut builder = build::Builder::new(ManifestConfig::default());
    let summary = WalkSummary {
        files: 0,
        errors: 0,
        outcome: WalkOutcome::Complete,
    };
    for record in records {
        builder.push(source, record);
    }
    builder.finish(source, summary)
}

#[test]
fn capture_does_not_depend_on_walk_order() {
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let (records, _) = records_and_summary(&source);
    let forward = built(&source, records.iter().cloned());
    let reversed = built(&source, records.iter().rev().cloned());
    let mut rotated = records.clone();
    rotated.rotate_left(records.len() / 2);
    let shifted = built(&source, rotated.into_iter());
    assert_eq!(forward.digest_hex(), reversed.digest_hex());
    assert_eq!(forward.digest_hex(), shifted.digest_hex());
    assert_eq!(Manifest::diff(&forward, &reversed).verdict, Verdict::Equal);
    assert_eq!(Manifest::diff(&forward, &shifted).verdict, Verdict::Equal);
    assert_eq!(forward.counts(), reversed.counts());
}

#[test]
fn content_budget_choice_does_not_depend_on_walk_order() {
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let (records, _) = records_and_summary(&source);
    let config = ManifestConfig {
        content_budget: 30,
        ..ManifestConfig::default()
    };
    let make = |iter: Vec<Record>| {
        let mut builder = build::Builder::new(config);
        iter.into_iter()
            .for_each(|record| builder.push(&source, record));
        let summary = source.walk(WalkConfig::default(), |_| {});
        builder.finish(&source, summary)
    };
    let mut backwards = records.clone();
    backwards.reverse();
    let (one, two) = (make(records), make(backwards));
    assert_eq!(one.digest_hex(), two.digest_hex());
    assert!(one.counts().hashed_bytes <= 30);
}

#[test]
fn capped_walk_is_incomplete_and_never_equal() {
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let config = ManifestConfig {
        walk: WalkConfig {
            max_files: 3,
            ..WalkConfig::default()
        },
        ..ManifestConfig::default()
    };
    let capped = Manifest::capture_with(&source, config);
    assert!(!capped.is_complete());
    let diff = Manifest::diff(&capped, &capped);
    assert_eq!(diff.verdict, Verdict::Incomplete);
    assert!(!diff.before_complete && !diff.after_complete);
}

#[test]
fn timed_out_walk_is_incomplete() {
    let fx = fixture::set().unwrap();
    let config = ManifestConfig {
        walk: WalkConfig {
            max_time: Duration::ZERO,
            ..WalkConfig::default()
        },
        ..ManifestConfig::default()
    };
    let timed = Manifest::capture_with(&source(&fx), config);
    assert!(!timed.is_complete());
    assert_eq!(Manifest::diff(&timed, &timed).verdict, Verdict::Incomplete);
}

#[test]
fn one_incomplete_side_is_enough_and_hides_nothing() {
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let full = Manifest::capture(&source);
    let config = ManifestConfig {
        walk: WalkConfig {
            max_files: 3,
            ..WalkConfig::default()
        },
        ..ManifestConfig::default()
    };
    let capped = Manifest::capture_with(&source, config);
    for diff in [
        Manifest::diff(&full, &capped),
        Manifest::diff(&capped, &full),
    ] {
        assert_eq!(diff.verdict, Verdict::Incomplete);
        assert!(diff.removed + diff.added > 0);
    }
}

#[cfg(unix)]
#[test]
fn unreadable_folder_makes_the_manifest_incomplete() {
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let (manifest, readable) = fx
        .with_unreadable_directory(|| {
            (
                Manifest::capture(&source),
                fs::read_dir(&fx.unreadable_dir).is_ok(),
            )
        })
        .unwrap();
    // A root user can read mode 000; then the walk is genuinely complete.
    assert_eq!(manifest.is_complete(), readable);
}

#[test]
fn big_and_over_budget_files_are_not_content_hashed() {
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let none = ManifestConfig {
        small_file_bytes: 4,
        ..ManifestConfig::default()
    };
    assert_eq!(
        Manifest::capture_with(&source, none).counts().hashed_bytes,
        0
    );
    let tight = ManifestConfig {
        content_budget: 25,
        ..ManifestConfig::default()
    };
    let counts = Manifest::capture_with(&source, tight).counts();
    assert!(counts.hashed_bytes <= 25);
}

#[test]
fn manifest_holds_only_flat_copy_records() {
    fn is_copy<T: Copy>() {}
    is_copy::<Entry>();
    assert!(std::mem::size_of::<Entry>() <= 128);
    let fx = fixture::set().unwrap();
    let source = source(&fx);
    let (records, summary) = records_and_summary(&source);
    let manifest = Manifest::capture(&source);
    assert_eq!(manifest.entries.len(), records.len());
    assert_eq!(summary.files, manifest.entries.len() as u64);
}

#[test]
fn no_name_or_path_reaches_debug_display_or_serde() {
    let fx = fixture::set().unwrap();
    let root = fx.root().to_string_lossy().into_owned();
    let before = capture(&fx);
    let to = fx.root().join("zqxjv-renamed-secret.txt");
    mutate::rename(&fx.privacy_file, &to).unwrap();
    mutate::add_file(&fx.root().join("zqxjv-added-secret.tmp")).unwrap();
    let after = capture(&fx);
    let diff = Manifest::diff(&before, &after);
    let mut text = format!("{before:?}{before}{after:?}{after}{diff:?}{diff}");
    text.push_str(&serde_json::to_string(&before).unwrap());
    text.push_str(&serde_json::to_string(&diff).unwrap());
    let needles = [
        fixture::MARKER,
        fixture::PRIVATE_FOLDER,
        fixture::PRIVATE_FILE,
        fixture::PRIVATE_EXTENSION,
        fixture::PRIVATE_QUERY,
        fixture::PRIVATE_CONTENT,
        fixture::LINK_NAME,
        "zqxjv-renamed-secret",
        "zqxjv-added-secret",
        "sample.nbq",
        "readonly.txt",
        "hard-link",
        ".hidden-entry",
        "unreadable",
        "segment-",
        &root,
    ];
    assert!(text.contains("changed"));
    assert_no_leaks(text.as_bytes(), &needles).unwrap();
}

#[test]
fn display_and_serde_carry_counts_and_verdict() {
    let fx = fixture::set().unwrap();
    let diff = diff_after(&fx, || mutate::add_file(&fx.root().join("x.tmp")));
    assert_eq!(
        diff.to_string(),
        "changed: 1 added, 0 removed, 0 renamed, 0 size, 0 mtime, 0 attributes, 0 content"
    );
    let json = serde_json::to_string(&diff).unwrap();
    assert!(json.contains("\"verdict\":\"changed\""));
}
