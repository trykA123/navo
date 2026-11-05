use std::sync::Arc;

use navo_core::ProviderPath;
use proptest::prelude::*;

use super::{Rows, copied_row_bytes, reset_copied_row_bytes};
use crate::view::{ViewFilter, filter_order, materialize};
use crate::{Attributes, CancelToken, EntryKind, FsEntry};

fn entry(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 0,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn rows_strategy() -> impl Strategy<Value = Vec<FsEntry>> {
    proptest::collection::vec(
        (
            "[a-z]{1,10}(\\.(txt|rs|md))?",
            any::<bool>(),
            any::<bool>(),
            0u64..10_000,
        )
            .prop_map(|(name, folder, hidden, size)| FsEntry {
                name: ProviderPath::from_text(&name),
                kind: if folder {
                    EntryKind::Folder
                } else {
                    EntryKind::File
                },
                size,
                modified_ms: None,
                attributes: Attributes {
                    hidden,
                    ..Attributes::default()
                },
            }),
        0..40,
    )
}

fn rows_and_order() -> impl Strategy<Value = (Vec<FsEntry>, Vec<u32>)> {
    rows_strategy().prop_flat_map(|rows| {
        let len = rows.len();
        proptest::collection::vec(any::<u32>(), len).prop_map(move |keys| {
            let mut order: Vec<u32> = (0..u32::try_from(len).unwrap()).collect();
            order.sort_by_key(|&index| (keys[index as usize], index));
            (rows.clone(), order)
        })
    })
}

fn row_bytes(entry: &FsEntry) -> usize {
    size_of::<FsEntry>() + entry.name.display().len()
}

proptest! {
    #[test]
    fn identity_matches_materialize((rows, _order) in rows_and_order()) {
        let source: Arc<[FsEntry]> = rows.clone().into();
        let view = Rows::all(Arc::clone(&source));
        let identity: Vec<u32> = (0..u32::try_from(rows.len()).unwrap()).collect();
        let expected = materialize(&rows, &identity);
        prop_assert!(Arc::ptr_eq(view.source(), &source));
        prop_assert!(view.order().is_none());
        prop_assert_eq!(view.len(), rows.len());
        prop_assert_eq!(view.is_empty(), rows.is_empty());
        prop_assert_eq!(view.to_vec(), expected.clone());
        prop_assert_eq!(view.iter().cloned().collect::<Vec<_>>(), expected.clone());
        prop_assert_eq!(view.iter().len(), rows.len());
        for (at, expected) in expected.iter().enumerate() {
            prop_assert_eq!(view.get(at), Some(expected));
            prop_assert_eq!(&view[at], expected);
        }
        prop_assert_eq!(view.get(rows.len()), None);
    }

    #[test]
    fn ordered_matches_materialize((rows, order) in rows_and_order()) {
        let source: Arc<[FsEntry]> = rows.clone().into();
        let view = Rows::ordered(source, order.clone().into());
        let expected = materialize(&rows, &order);
        prop_assert_eq!(view.len(), order.len());
        prop_assert_eq!(view.to_vec(), expected.clone());
        prop_assert_eq!(view.iter().cloned().collect::<Vec<_>>(), expected.clone());
        prop_assert_eq!(view.iter().len(), order.len());
        prop_assert_eq!(
            view.iter().rev().cloned().collect::<Vec<_>>(),
            expected.iter().rev().cloned().collect::<Vec<_>>()
        );
        for (at, expected) in expected.iter().enumerate() {
            prop_assert_eq!(view.get(at), Some(expected));
            prop_assert_eq!(&view[at], expected);
        }
        prop_assert_eq!(view.get(order.len()), None);
    }

    #[test]
    fn narrowed_after_filter_matches_materialize(
        (rows, order) in rows_and_order(),
        show_hidden in any::<bool>(),
        show_system in any::<bool>(),
    ) {
        let filter = ViewFilter {
            needle: None,
            show_hidden,
            show_system,
        };
        let keep = filter_order(&rows, &order, &filter, &CancelToken::new()).unwrap();
        let view = Rows::ordered(rows.clone().into(), order.into()).narrowed(keep.clone());
        let expected = materialize(&rows, &keep);
        prop_assert_eq!(view.len(), keep.len());
        prop_assert_eq!(view.to_vec(), expected.clone());
        prop_assert_eq!(view.iter().cloned().collect::<Vec<_>>(), expected.clone());
        for (at, entry) in expected.iter().enumerate() {
            prop_assert_eq!(view.get(at), Some(entry));
        }
    }
}

#[test]
fn to_arc_on_identity_shares_the_source_without_copying() {
    let rows = vec![entry("alpha.txt"), entry("beta.rs")];
    let source: Arc<[FsEntry]> = rows.into();
    let view = Rows::all(Arc::clone(&source));
    reset_copied_row_bytes();
    let shared = view.to_arc();
    assert!(Arc::ptr_eq(&shared, &source));
    assert_eq!(copied_row_bytes(), 0);
}

#[test]
fn ordered_copies_meter_exactly_the_row_bytes() {
    let rows = vec![entry("alpha.txt"), entry("beta.rs"), entry("gamma")];
    let order = vec![2, 0, 1];
    let expected: usize = order.iter().map(|&at| row_bytes(&rows[at as usize])).sum();
    let view = Rows::ordered(rows.clone().into(), order.clone().into());

    reset_copied_row_bytes();
    let copied = view.to_vec();
    assert_eq!(copied_row_bytes(), expected);
    assert_eq!(copied, materialize(&rows, &order));

    reset_copied_row_bytes();
    assert_eq!(materialize(&rows, &order), copied);
    assert_eq!(copied_row_bytes(), expected);

    reset_copied_row_bytes();
    assert_eq!(&*view.to_arc(), &copied[..]);
    assert_eq!(copied_row_bytes(), expected);
}

#[test]
fn same_compares_source_and_order_identity() {
    let source: Arc<[FsEntry]> = vec![entry("a"), entry("b")].into();
    let order: Arc<[u32]> = vec![1, 0].into();
    let all = Rows::all(Arc::clone(&source));
    assert!(Rows::same(&all, &Rows::all(Arc::clone(&source))));
    let ordered = Rows::ordered(Arc::clone(&source), Arc::clone(&order));
    assert!(Rows::same(
        &ordered,
        &Rows::ordered(source, Arc::clone(&order))
    ));
    assert!(!Rows::same(&all, &ordered));
    assert!(!Rows::same(&all, &all.narrowed(vec![0, 1])));
}

#[test]
fn weak_rows_match_their_source_after_the_view_drops() {
    let source_a: Arc<[FsEntry]> = vec![entry("a")].into();
    let source_b: Arc<[FsEntry]> = vec![entry("b")].into();
    let rows_a = Rows::all(source_a);
    let rows_b = Rows::all(source_b);
    let weak = rows_a.downgrade();
    assert!(weak.is_view_of(&rows_a));
    assert!(!weak.is_view_of(&rows_b));
    drop(rows_a);
    assert!(!weak.is_view_of(&rows_b));
}

#[test]
fn weak_rows_tell_views_of_one_source_apart_by_order() {
    let source: Arc<[FsEntry]> = vec![entry("a"), entry("b")].into();
    let all = Rows::all(Arc::clone(&source));
    let narrowed = all.narrowed(vec![0]);
    let other = all.narrowed(vec![1]);
    let weak_all = all.downgrade();
    assert!(weak_all.is_view_of(&all));
    assert!(!weak_all.is_view_of(&narrowed));
    let weak_narrowed = narrowed.downgrade();
    assert!(weak_narrowed.is_view_of(&narrowed));
    assert!(!weak_narrowed.is_view_of(&all));
    assert!(!weak_narrowed.is_view_of(&other));
}
