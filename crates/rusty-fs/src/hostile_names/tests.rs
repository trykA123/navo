use std::collections::HashSet;
use std::ops::ControlFlow;

use rusty_core::{Location, ProviderId, ProviderPath};

use super::*;
use crate::{Attributes, CancelToken, EntryBatch, EntryKind, FakeFs, FsEntry, Provider};

const WINDOWS_ILLEGAL: [u16; 9] = [
    b'\\' as u16,
    b':' as u16,
    b'*' as u16,
    b'?' as u16,
    b'"' as u16,
    b'<' as u16,
    b'>' as u16,
    b'|' as u16,
    b'/' as u16,
];

fn labels_where(pred: impl Fn(&HostileName) -> bool) -> Vec<&'static str> {
    corpus()
        .into_iter()
        .filter(|n| pred(n))
        .map(|n| n.label)
        .collect()
}

fn entry(index: usize, name: ProviderPath) -> FsEntry {
    FsEntry {
        name,
        kind: if index.is_multiple_of(2) {
            EntryKind::File
        } else {
            EntryKind::Folder
        },
        size: 0,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn filled_fake() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("h");
    for (i, n) in corpus().into_iter().enumerate() {
        fs.add_entry("h", entry(i, n.name));
    }
    fs
}

fn listed(fs: &FakeFs) -> Vec<ProviderPath> {
    let mut names = Vec::new();
    let mut sink = |batch: EntryBatch| {
        names.extend(batch.entries.into_iter().map(|e| e.name));
        ControlFlow::Continue(())
    };
    fs.list(&fs.location("h"), &mut sink, &CancelToken::new())
        .unwrap();
    names
}

fn fake_id() -> ProviderId {
    ProviderId::new("fake").unwrap()
}

#[test]
fn labels_are_unique() {
    let all = corpus();
    let labels: HashSet<_> = all.iter().map(|n| n.label).collect();
    assert_eq!(labels.len(), all.len());
}

#[test]
fn names_are_unique() {
    let all = corpus();
    let names: HashSet<_> = all.iter().map(|n| n.name.clone()).collect();
    assert_eq!(names.len(), all.len());
}

#[test]
fn every_name_is_one_non_empty_segment() {
    let bad = labels_where(|n| {
        let text = n.name.to_text();
        n.name.is_empty() || text.contains('\0') || text.contains('/')
    });
    assert_eq!(bad, Vec::<&str>::new());
}

#[test]
fn every_name_exists_on_some_family() {
    assert_eq!(labels_where(|n| !n.unix && !n.windows), Vec::<&str>::new());
}

#[test]
fn windows_names_hold_no_illegal_character() {
    let bad = labels_where(|n| {
        n.windows
            && n.name.to_wide().is_none_or(|units| {
                units
                    .iter()
                    .any(|u| *u < 0x20 || WINDOWS_ILLEGAL.contains(u))
            })
    });
    assert_eq!(bad, Vec::<&str>::new());
}

#[test]
fn unix_names_fit_in_255_bytes() {
    let bad = labels_where(|n| n.unix && n.name.to_bytes().is_none_or(|b| b.len() > MAX_NAME));
    assert_eq!(bad, Vec::<&str>::new());
}

#[test]
fn windows_names_fit_in_255_utf16_units() {
    let bad = labels_where(|n| n.windows && n.name.to_wide().is_none_or(|w| w.len() > MAX_NAME));
    assert_eq!(bad, Vec::<&str>::new());
}

#[test]
fn unix_family_is_the_unix_flag() {
    let expected: Vec<_> = corpus().into_iter().filter(|n| n.unix).collect();
    assert_eq!(for_family(Family::Unix), expected);
}

#[test]
fn windows_family_is_the_windows_flag() {
    let expected: Vec<_> = corpus().into_iter().filter(|n| n.windows).collect();
    assert_eq!(for_family(Family::Windows), expected);
}

#[test]
fn families_leave_out_names_of_the_other_system() {
    let unix = for_family(Family::Unix);
    let windows = for_family(Family::Windows);
    assert!(unix.len() < corpus().len() && windows.len() < corpus().len());
}

#[test]
fn long_path_segments_pass_260_utf16_units() {
    let units: usize = long_path_segments()
        .iter()
        .map(|s| s.to_wide().map_or(0, |w| w.len() + 1))
        .sum();
    assert!(units > 260, "{units}");
}

#[test]
fn long_path_segments_are_single_segments() {
    let bad = long_path_segments()
        .iter()
        .filter(|s| s.is_empty() || s.to_text().contains('/'))
        .count();
    assert_eq!(bad, 0);
}

#[test]
fn fake_lists_every_name_in_insertion_order() {
    let expected: Vec<_> = corpus().into_iter().map(|n| n.name).collect();
    assert_eq!(listed(&filled_fake()), expected);
}

#[test]
fn fake_listing_keeps_bytes_and_units() {
    let forms = |p: &ProviderPath| (p.to_bytes(), p.to_wide());
    let expected: Vec<_> = corpus().iter().map(|n| forms(&n.name)).collect();
    let got: Vec<_> = listed(&filled_fake()).iter().map(forms).collect();
    assert_eq!(got, expected);
}

#[test]
fn fake_metadata_returns_every_name_unchanged() {
    let fs = filled_fake();
    let wrong: Vec<_> = corpus()
        .into_iter()
        .filter_map(|n| {
            let location = fs.location(&format!("h/{}", n.name.to_text()));
            let got = fs.metadata(&location).map(|e| e.name);
            (got.as_ref() != Ok(&n.name)).then(|| (n.label, got.map_err(|e| e.kind)))
        })
        .collect();
    assert_eq!(wrong, Vec::new());
}

#[test]
fn text_form_round_trips_every_name() {
    let wrong: Vec<_> = corpus()
        .into_iter()
        .filter_map(|n| {
            let back = ProviderPath::from_text(&n.name.to_text());
            (back != n.name).then_some((n.label, back))
        })
        .collect();
    assert_eq!(wrong, Vec::new());
}

#[test]
fn location_text_survives_parsing_for_every_name() {
    let wrong: Vec<_> = corpus()
        .into_iter()
        .filter_map(|n| {
            let path = ProviderPath::from_text(&format!("h/{}", n.name.to_text()));
            let loc = Location::with_provider(fake_id(), path).unwrap();
            let back = Location::new(loc.as_str());
            (back.as_ref() != Ok(&loc)).then_some((n.label, back))
        })
        .collect();
    assert_eq!(wrong, Vec::new());
}

#[test]
fn display_is_never_empty() {
    assert_eq!(
        labels_where(|n| n.name.display().is_empty()),
        Vec::<&str>::new()
    );
}

#[test]
fn display_holds_no_nul() {
    assert_eq!(
        labels_where(|n| n.name.display().contains('\0')),
        Vec::<&str>::new()
    );
}
