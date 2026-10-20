use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use super::{TableHit, Term, search};
use crate::fuzzy::Fuzzy;
use crate::table::{Flags, NO_PARENT, NameTable};

fn files(names: &[&str]) -> NameTable {
    let mut table = NameTable::new();
    for name in names {
        table.push(NO_PARENT, name, Flags::NONE).unwrap();
    }
    table
}

fn ids(hits: &[TableHit]) -> Vec<u32> {
    hits.iter().map(|hit| hit.id).collect()
}

#[test]
fn search_requires_all_name_terms_and_combines_name_indices() {
    let table = files(&["readme.rs", "read-only.rs", "memo.rs"]);
    let hits = search(&table, &[Term::Name("read"), Term::Name("me")], 10, &|| {
        false
    });
    assert_eq!(ids(&hits), vec![0]);
    assert_eq!(hits[0].indices, vec![0, 1, 2, 3, 4, 5]);
    let first = Fuzzy::new("read", false).score("readme.rs").unwrap();
    let second = Fuzzy::new("me", false).score("readme.rs").unwrap();
    assert_eq!(hits[0].score, first.score + second.score);
}

#[test]
fn search_name_terms_do_not_match_parents() {
    let mut table = NameTable::new();
    let folder = table.push(NO_PARENT, "project", Flags::FOLDER).unwrap();
    table.push(folder, "readme.rs", Flags::NONE).unwrap();
    let hits = search(&table, &[Term::Name("project")], 10, &|| false);
    assert_eq!(ids(&hits), vec![folder]);
}

#[test]
fn search_path_terms_match_parents_after_name_terms() {
    let mut table = NameTable::new();
    let project = table.push(NO_PARENT, "project", Flags::FOLDER).unwrap();
    let source = table.push(project, "src", Flags::FOLDER).unwrap();
    let expected = table.push(source, "readme.rs", Flags::NONE).unwrap();
    let other = table.push(NO_PARENT, "other", Flags::FOLDER).unwrap();
    table.push(other, "readme.rs", Flags::NONE).unwrap();
    table.push(source, "notes.rs", Flags::NONE).unwrap();
    let terms = [Term::Path("project/src"), Term::Name("readme")];
    let hits = search(&table, &terms, 10, &|| false);
    assert_eq!(ids(&hits), vec![expected]);
    assert_eq!(hits[0].indices, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn search_path_only_highlights_name_characters_with_either_separator() {
    for parent in ["données", "日本e\u{301}"] {
        let mut table = NameTable::new();
        let folder = table.push(NO_PARENT, parent, Flags::FOLDER).unwrap();
        let expected = table.push(folder, "résumé.rs", Flags::NONE).unwrap();
        for separator in ['/', '\\'] {
            let term = format!("{parent}{separator}résumé");
            let hits = search(&table, &[Term::Path(&term)], 10, &|| false);
            assert_eq!(ids(&hits), vec![expected]);
            assert_eq!(hits[0].indices, vec![0, 1, 2, 3, 4, 5]);
        }
    }
}

#[test]
fn search_extension_filter_is_case_insensitive_and_accepts_alternatives() {
    let mut table = files(&["readme.RS", "readme.toml", "readme.txt", "readme.rs.bak"]);
    table.push(NO_PARENT, "readme.rs", Flags::FOLDER).unwrap();
    let extensions = vec!["rs".into(), "toml".into()];
    let terms = [Term::Name("readme"), Term::Ext(&extensions)];
    let hits = search(&table, &terms, 10, &|| false);
    let mut actual = ids(&hits);
    actual.sort_unstable();
    assert_eq!(actual, vec![0, 1]);
}

#[test]
fn search_repeated_extension_filters_require_both() {
    let table = files(&["config.rs", "config.toml", "config.txt"]);
    let first = vec!["rs".into(), "toml".into()];
    let second = vec!["toml".into(), "txt".into()];
    let hits = search(
        &table,
        &[Term::Ext(&first), Term::Ext(&second)],
        10,
        &|| false,
    );
    assert_eq!(ids(&hits), vec![1]);
    assert_eq!(hits[0].score, 0);
    assert!(hits[0].indices.is_empty());
}

#[test]
fn search_extension_filter_does_not_treat_dotfiles_or_suffixes_as_extensions() {
    let table = files(&[".rs", "notes.rs", "notesrs", "notes.rs.bak", "notes."]);
    let extensions = vec!["rs".into()];
    let hits = search(&table, &[Term::Ext(&extensions)], 10, &|| false);
    assert_eq!(ids(&hits), vec![1]);
}

#[test]
fn search_skips_deleted_entries_and_scans_later_slots() {
    let mut table = files(&["readme.rs", "readme.rs", "readme.rs"]);
    table.remove(0);
    table.remove(1);
    let hits = search(&table, &[Term::Name("readme")], 10, &|| false);
    assert_eq!(ids(&hits), vec![2]);
}

#[test]
fn search_pre_cancelled_returns_no_hits() {
    let table = files(&["readme.rs"]);
    assert!(search(&table, &[Term::Name("readme")], 10, &|| true).is_empty());
}

#[test]
fn search_cancellation_stops_before_scanning_the_whole_chunk() {
    let table = files(&["readme.rs"; 100]);
    let polls = AtomicUsize::new(0);
    let cancel = || polls.fetch_add(1, Ordering::Relaxed) >= 12;
    let hits = search(&table, &[Term::Name("readme")], 10, &cancel);
    assert!(hits.is_empty());
    assert!((13..20).contains(&polls.load(Ordering::Relaxed)));
}

#[test]
fn search_equal_scores_keep_table_order_across_chunks() {
    let mut table = NameTable::new();
    let mut expected = Vec::new();
    for id in 0..140_000 {
        let name = if matches!(id, 7 | 65_537 | 131_073) {
            expected.push(id);
            "readme.rs"
        } else {
            "other.txt"
        };
        table.push(NO_PARENT, name, Flags::NONE).unwrap();
    }
    for _ in 0..3 {
        let hits = search(&table, &[Term::Name("readme")], 2, &|| false);
        assert_eq!(ids(&hits), expected[..2]);
        assert_eq!(hits[0].score, hits[1].score);
    }
}

#[test]
fn search_keeps_the_highest_scores_within_the_limit() {
    let table = files(&["x_r_e_a_d_m_e.rs", "readme.rs", "a_readme.rs"]);
    let all = search(&table, &[Term::Name("readme")], 10, &|| false);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].id, 1);
    assert!(all.windows(2).all(|pair| pair[0].score >= pair[1].score));
    let limited = search(&table, &[Term::Name("readme")], 2, &|| false);
    assert_eq!(limited, all[..2]);
}

#[test]
fn search_empty_query_keeps_live_table_order() {
    let mut table = files(&["one", "two", "three"]);
    table.remove(1);
    let hits = search(&table, &[], 10, &|| false);
    assert_eq!(ids(&hits), vec![0, 2]);
    assert!(
        hits.iter()
            .all(|hit| hit.score == 0 && hit.indices.is_empty())
    );
}

#[test]
fn search_zero_limit_and_empty_table_return_no_hits() {
    let table = files(&["readme.rs"]);
    assert!(search(&table, &[Term::Name("readme")], 0, &|| false).is_empty());
    assert!(search(&NameTable::new(), &[], 10, &|| false).is_empty());
}

#[test]
#[ignore = "informational release benchmark on one million synthetic names"]
#[allow(
    clippy::print_stdout,
    reason = "explicit benchmark reports its measurement"
)]
fn search_million_names_timing() {
    let mut table = NameTable::with_capacity(1_000_000, 24_000_000);
    for id in 0..1_000_000 {
        let name = format!("document-{id:07}.rs");
        table.push(NO_PARENT, &name, Flags::NONE).unwrap();
    }
    let terms = [Term::Name("doc")];
    let expected = search(&table, &terms, 50, &|| false);
    let started = Instant::now();
    let hits = search(&table, &terms, 50, &|| false);
    let elapsed = started.elapsed();
    assert_eq!(hits.len(), 50);
    assert_eq!(hits, expected);
    println!("search: 1,000,000 synthetic names, limit 50: {elapsed:?}");
}
