//! Tests for the gutter labels: every status, the folder rules and the l10n words.

use super::{GutterText, gutter_label};
use crate::browser::{CompareKind, CompareRow, CompareSide, RowProgress, RowStatus, Tone};

const SIDE: CompareSide = CompareSide {
    kind: CompareKind::File,
    size: 1,
    modified_ms: None,
};
const DIR: CompareSide = CompareSide {
    kind: CompareKind::Folder,
    ..SIDE
};

fn file(status: RowStatus) -> CompareRow {
    CompareRow::new(1, "a.txt", status, (Some(SIDE), Some(SIDE)))
}

fn folder(status: RowStatus, inside: u32) -> CompareRow {
    let mut row = CompareRow::new(2, "d", status, (Some(DIR), Some(DIR)));
    row.contains_differences = inside > 0;
    row.differences_inside = inside;
    row
}

fn label(row: &CompareRow) -> Option<(GutterText, Tone, String)> {
    gutter_label(row, None).map(|l| (l.text, l.tone, l.hover))
}

#[test]
fn every_status_maps_to_its_label_tone_and_hover() {
    let cases = [
        (RowStatus::Same, None),
        (RowStatus::SameSizeTime, None),
        (
            RowStatus::SameAfterRules,
            Some((GutterText::Rules, Tone::Muted, "same after rules")),
        ),
        (
            RowStatus::Different,
            Some((GutterText::Changed, Tone::Change, "different")),
        ),
        (
            RowStatus::OnlyLeft,
            Some((GutterText::OnlyLeft, Tone::Delete, "only left")),
        ),
        (
            RowStatus::OnlyRight,
            Some((GutterText::OnlyRight, Tone::Add, "only right")),
        ),
        (
            RowStatus::TypeConflict,
            Some((GutterText::Type, Tone::Conflict, "type conflict")),
        ),
        (
            RowStatus::CaseConflict,
            Some((GutterText::Type, Tone::Conflict, "case conflict")),
        ),
    ];
    for (status, want) in cases {
        let want = want.map(|(text, tone, hover)| (text, tone, hover.to_owned()));
        assert_eq!(label(&file(status)), want, "{status:?}");
    }
}

#[test]
fn a_checking_row_shows_its_percent() {
    let progress = RowProgress {
        id: 1,
        bytes_done: 250,
        bytes_total: 1000,
    };
    let got = gutter_label(&file(RowStatus::Checking), Some(progress)).expect("label");
    assert_eq!((got.text, got.tone), (GutterText::Checking(25), Tone::Busy));
    assert!(got.hover.starts_with("same size and time"), "{}", got.hover);
}

#[test]
fn a_checking_row_without_progress_says_pending() {
    let got = gutter_label(&file(RowStatus::Checking), None).expect("label");
    assert_eq!(
        (got.text, got.tone, got.hover.as_str()),
        (
            GutterText::Pending,
            Tone::Muted,
            "same size and time · checking"
        )
    );
}

#[test]
fn a_checking_row_with_total_zero_says_pending() {
    let progress = RowProgress {
        id: 1,
        bytes_done: 0,
        bytes_total: 0,
    };
    let got = gutter_label(&file(RowStatus::Checking), Some(progress)).expect("label");
    assert_eq!(
        (got.text, got.tone, got.hover.as_str()),
        (
            GutterText::Pending,
            Tone::Muted,
            "same size and time · checking"
        )
    );
}

#[test]
fn an_unchecked_row_says_not_checked() {
    let got =
        gutter_label(&file(RowStatus::NotChecked("compare-reason-stopped")), None).expect("label");
    assert_eq!(
        (got.text, got.tone, got.hover.as_str()),
        (GutterText::NotChecked, Tone::Muted, "Not checked (stopped)")
    );
}

#[test]
fn an_unavailable_row_says_cant_read() {
    let got = gutter_label(
        &file(RowStatus::Unavailable("compare-reason-access-denied")),
        None,
    )
    .expect("label");
    assert_eq!(
        (got.text, got.tone, got.hover.as_str()),
        (
            GutterText::Unavailable,
            Tone::Muted,
            "unavailable: access denied"
        )
    );
}

#[test]
fn a_same_folder_with_differences_inside_says_the_count() {
    let got = label(&folder(RowStatus::Same, 3));
    let hover = "contains differences".to_owned();
    assert_eq!(got, Some((GutterText::Inside(3), Tone::Change, hover)));
}

#[test]
fn a_changed_folder_with_differences_inside_says_the_count_too() {
    let got = label(&folder(RowStatus::Different, 7)).expect("label");
    assert_eq!((got.0, got.1), (GutterText::Inside(7), Tone::Change));
}

#[test]
fn a_folder_without_a_count_falls_back_to_its_own_status() {
    assert_eq!(label(&folder(RowStatus::Same, 0)), None);
    let mut pending = folder(RowStatus::Same, 0);
    pending.contains_differences = true;
    let got = label(&pending).expect("label");
    assert_eq!((got.0, got.1), (GutterText::Changed, Tone::Change));
    let one_sided = folder(RowStatus::OnlyLeft, 4);
    assert_eq!(label(&one_sided).expect("label").0, GutterText::OnlyLeft);
}

#[test]
fn the_words_come_from_the_l10n_ids() {
    let cases = [
        (GutterText::Changed, "changed"),
        (GutterText::OnlyLeft, "◀ only"),
        (GutterText::OnlyRight, "only ▶"),
        (GutterText::Type, "type ≠"),
        (GutterText::Inside(12), "12 inside"),
        (GutterText::Rules, "same*"),
        (GutterText::Checking(25), "25%"),
        (GutterText::Pending, "…"),
        (GutterText::NotChecked, "unchecked"),
        (GutterText::Unavailable, "can't read"),
    ];
    for (text, words) in cases {
        assert_eq!(text.words(), words, "{}", text.id());
    }
}
