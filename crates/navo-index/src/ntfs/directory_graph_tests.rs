//! Tests for T2 folder-only parent resolution, root normalization and cycle/conflict checks.

use super::directory_graph::{DirectoryGraph, DirectoryRecord};
use super::{Flags, NtfsError, NtfsIndex, ROOT_REFERENCE as ROOT};
use crate::usn::JournalPosition;

const F: Flags = Flags::FOLDER;
const POS: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

fn build(entries: &[(u128, u128, &str, Flags)]) -> Result<NtfsIndex, NtfsError> {
    build_counted(entries).map(|(index, _)| index)
}

/// Builds the folder index and returns the number of skipped records.
fn build_counted(entries: &[(u128, u128, &str, Flags)]) -> Result<(NtfsIndex, u64), NtfsError> {
    let mut graph = DirectoryGraph::new();
    for &(file_id, parent_id, name, flags) in entries {
        graph.push(DirectoryRecord {
            file_id,
            parent_id,
            name: name.into(),
            flags,
        })?;
    }
    let mut index = NtfsIndex::new('C', POS)?;
    let resolved = graph.insert_into(&mut index)?;
    Ok((index, resolved.skipped + resolved.unplaced.len() as u64))
}

fn path(index: &NtfsIndex, file_id: u128) -> Option<String> {
    index
        .ids
        .get(&file_id)
        .and_then(|&id| index.table.path_with(id, '\\'))
}

#[test]
fn directory_graph_inserts_reverse_depth_and_deep_chain_parent_first() {
    let idx = build(&[
        (40, 30, "child", F),
        (30, 20, "parent", F),
        (20, ROOT, "gp", F),
    ])
    .unwrap();
    assert_eq!(path(&idx, 40).as_deref(), Some("C:\\gp\\parent\\child"));
    assert_eq!(idx.table.len(), 4);
    let deep: Vec<_> = (0..4500u128)
        .rev()
        .map(|i| (100 + i, if i == 0 { ROOT } else { 99 + i }, "d", F))
        .collect();
    assert_eq!(build(&deep).unwrap().table.len(), 4501);
}

#[test]
fn directory_graph_normalizes_root_sequence_aliases() {
    let seq = (5 << 48) | 5;
    let idx = build(&[
        (seq, seq, ".", F | Flags::SYSTEM),
        (10, seq, "a", F),
        (11, (9 << 48) | 5, "b", F),
    ])
    .unwrap();
    assert_eq!(idx.ids.get(&seq), idx.ids.get(&ROOT));
    assert_eq!(idx.table.name(0), Some("C:"));
    assert!(
        idx.table
            .flags(0)
            .is_some_and(|f| f.contains(F | Flags::SYSTEM))
    );
    assert_eq!(path(&idx, 10).as_deref(), Some("C:\\a"));
    assert_eq!(path(&idx, 11).as_deref(), Some("C:\\b"));
}

#[test]
fn directory_graph_skips_conflicts_and_invalid_parent_chains() {
    for (records, expected_skipped, expected_path) in [
        (
            &[(10, ROOT, "d", F), (10, ROOT, "d", F)][..],
            0,
            Some("C:\\d"),
        ),
        (
            &[(10, ROOT, "d", F), (10, ROOT, "other", F)],
            1,
            Some("C:\\d"),
        ),
        (&[(10, ROOT, "d", F), (10, 20, "d", F)], 1, Some("C:\\d")),
        (
            &[(10, ROOT, "d", F), (10, ROOT, "d", F | Flags::HIDDEN)],
            1,
            Some("C:\\d"),
        ),
        (&[(10, 999, "orphan", F)], 1, None),
        (&[(10, 10, "self", F)], 1, None),
        (
            &[(10, 20, "a", F), (20, 10, "b", F), (30, 10, "c", F)],
            3,
            None,
        ),
    ] {
        let (index, skipped) = build_counted(records).unwrap();
        assert_eq!(skipped, expected_skipped);
        assert_eq!(path(&index, 10).as_deref(), expected_path);
        if expected_path.is_some() {
            assert_eq!(index.table.flags(index.ids[&10]), Some(F));
        }
    }
}

/// Counts both unreachable folders even when the child appears first.
#[test]
fn directory_graph_skips_an_orphan_folder_and_its_child() {
    let (index, skipped) = build_counted(&[(11, 10, "child", F), (10, 999, "orphan", F)]).unwrap();
    assert_eq!(skipped, 2);
    assert_eq!(index.table.len(), 1);
}
