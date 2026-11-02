use std::ops::Range;
use std::path::{Path, PathBuf};

use super::{Field, Found, Item, MAX_RANGES, Mode, Rows, join_paths, search};
use crate::model::{Content, Entry, Saved, SavedKind, Scope, Transfer};
use crate::store::Stored;

fn text_entry(id: i64, body: &str) -> Stored {
    Stored {
        id,
        entry: Entry {
            content: Content::Text(body.to_owned()),
            source_app: None,
            first_ms: 0,
            last_ms: 0,
            uses: 1,
        },
    }
}

fn paths_entry(id: i64, paths: &[&str]) -> Stored {
    Stored {
        id,
        entry: Entry {
            content: Content::Paths {
                paths: paths.iter().map(PathBuf::from).collect(),
                transfer: Transfer::Copy,
            },
            source_app: None,
            first_ms: 0,
            last_ms: 0,
            uses: 1,
        },
    }
}

fn saved(id: i64, kind: SavedKind, title: &str, body: Option<&str>) -> Saved {
    Saved {
        id,
        kind,
        title: title.to_owned(),
        body: body.map(str::to_owned),
        user: None,
        sealed: None,
        scope: Scope::Global,
        created_ms: 0,
        updated_ms: 0,
    }
}

fn secret(id: i64, title: &str, body: &str, user: &str) -> Saved {
    Saved {
        user: Some(user.to_owned()),
        ..saved(id, SavedKind::Secret, title, Some(body))
    }
}

fn note(id: i64, title: &str, folder: Option<&str>) -> Saved {
    Saved {
        scope: match folder {
            Some(folder) => Scope::Folder(PathBuf::from(folder)),
            None => Scope::Global,
        },
        ..saved(id, SavedKind::Note, title, Some("note body"))
    }
}

fn id(found: &Found<'_>) -> i64 {
    match found.item {
        Item::History(stored) => stored.id,
        Item::Saved(saved) => saved.id,
    }
}

fn ids(found: &[Found<'_>]) -> Vec<i64> {
    found.iter().map(id).collect()
}

fn assert_slices(text: &str, ranges: &[Range<usize>], expected: &[&str]) {
    for range in ranges {
        assert!(text.is_char_boundary(range.start), "{range:?}");
        assert!(text.is_char_boundary(range.end), "{range:?}");
    }
    let slices: Vec<&str> = ranges.iter().map(|range| &text[range.clone()]).collect();
    assert_eq!(slices, expected);
}

#[test]
fn all_mode_lists_history_then_saved_in_order() {
    let history = vec![text_entry(2, "two"), text_entry(1, "one")];
    let saved = vec![
        saved(10, SavedKind::Snippet, "snippet", None),
        saved(11, SavedKind::Command, "command", None),
        saved(12, SavedKind::Path, "path", None),
        secret(13, "secret", "sealed", "user"),
        note(14, "note", None),
    ];
    let rows = Rows {
        history: &history,
        saved: &saved,
    };
    let found = search(rows, Mode::All, "", None);
    assert_eq!(ids(&found), vec![2, 1, 10, 11, 12, 13, 14]);
}

#[test]
fn empty_query_returns_every_row_of_the_mode_in_order() {
    let history = vec![text_entry(1, "one"), paths_entry(2, &["a"])];
    let saved = vec![
        saved(10, SavedKind::Snippet, "snippet", None),
        secret(11, "secret", "sealed", "user"),
        note(12, "note", None),
    ];
    let rows = Rows {
        history: &history,
        saved: &saved,
    };
    assert_eq!(
        ids(&search(rows, Mode::All, "", None)),
        vec![1, 2, 10, 11, 12]
    );
    assert_eq!(ids(&search(rows, Mode::Recent, "", None)), vec![1, 2]);
    assert_eq!(ids(&search(rows, Mode::Saved, "", None)), vec![10]);
    assert_eq!(ids(&search(rows, Mode::Secrets, "", None)), vec![11]);
    assert_eq!(ids(&search(rows, Mode::Notes, "", None)), vec![12]);
    assert!(
        search(rows, Mode::All, "", None)
            .iter()
            .all(|found| found.field.is_none() && found.ranges.is_empty())
    );
}

#[test]
fn recent_mode_matches_history_text_and_path_lists_only() {
    let history = vec![
        text_entry(1, "Alpha release"),
        paths_entry(2, &["C:\\one\\alpha.txt", "C:\\two\\beta.txt"]),
    ];
    let saved = vec![saved(10, SavedKind::Snippet, "alpha", Some("alpha"))];
    let rows = Rows {
        history: &history,
        saved: &saved,
    };
    let found = search(rows, Mode::Recent, "ALPHA", None);
    assert_eq!(ids(&found), vec![1, 2]);
    assert_eq!(found[0].field, Some(Field::Body));
    assert_eq!(found[1].field, Some(Field::Paths));
}

#[test]
fn saved_mode_skips_secrets_and_notes() {
    let saved = vec![
        saved(10, SavedKind::Snippet, "shared", Some("needle")),
        secret(11, "shared", "needle", "shared"),
        note(12, "shared", None),
        saved(13, SavedKind::Command, "shared", None),
    ];
    let found = search(
        Rows {
            history: &[],
            saved: &saved,
        },
        Mode::Saved,
        "shared",
        None,
    );
    assert_eq!(ids(&found), vec![10, 13]);
}

#[test]
fn saved_items_match_title_and_body() {
    let saved = vec![saved(
        1,
        SavedKind::Snippet,
        "title-needle",
        Some("body-needle"),
    )];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let by_title = search(rows, Mode::Saved, "title-needle", None);
    assert_eq!(by_title[0].field, Some(Field::Title));
    let by_body = search(rows, Mode::Saved, "body-needle", None);
    assert_eq!(by_body[0].field, Some(Field::Body));
}

#[test]
fn secrets_mode_matches_title_and_user_name() {
    let saved = vec![secret(11, "GitHub", "canary-body", "octocat")];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let by_title = search(rows, Mode::Secrets, "github", None);
    assert_eq!(by_title[0].field, Some(Field::Title));
    let by_user = search(rows, Mode::Secrets, "octo", None);
    assert_eq!(by_user[0].field, Some(Field::User));
    assert_slices("octocat", &by_user[0].ranges, &["octo"]);
}

#[test]
fn a_secret_body_never_matches() {
    let saved = vec![
        secret(11, "login", "canary-body", "alice"),
        saved(12, SavedKind::Snippet, "other", Some("canary-body")),
    ];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let found = search(rows, Mode::All, "canary", None);
    assert_eq!(ids(&found), vec![12]);
    assert!(search(rows, Mode::Secrets, "canary", None).is_empty());
}

#[test]
fn a_note_for_the_current_folder_sorts_first() {
    let saved = vec![
        note(1, "global", None),
        note(2, "other", Some("D:\\Other")),
        note(3, "here", Some("D:\\Projects\\Ecu-A2")),
        saved(4, SavedKind::Snippet, "here", None),
    ];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let found = search(rows, Mode::Notes, "", Some(Path::new("d:/projects/ECU-a2")));
    assert_eq!(ids(&found), vec![3, 1, 2]);
    let no_folder = search(rows, Mode::Notes, "", None);
    assert_eq!(ids(&no_folder), vec![1, 2, 3]);
}

#[test]
fn path_lists_match_the_paths_joined_by_newlines() {
    let paths = ["C:\\one\\alpha.txt", "C:\\two\\beta.txt"];
    let history = vec![paths_entry(7, &paths)];
    let found = search(
        Rows {
            history: &history,
            saved: &[],
        },
        Mode::Recent,
        "one\\alpha.txt\nC:\\two",
        None,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].field, Some(Field::Paths));
    let joined = join_paths(&history_paths(&history[0]));
    assert_slices(&joined, &found[0].ranges, &["one\\alpha.txt\nC:\\two"]);
}

#[test]
fn highlight_ranges_stay_on_original_character_boundaries() {
    let saved = vec![saved(1, SavedKind::Snippet, "İstanbul ẞtraße", None)];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let title = &saved[0].title;

    let dotted = search(rows, Mode::All, "İSTANBUL", None);
    assert_slices(title, &dotted[0].ranges, &["İstanbul"]);

    let sharp = search(rows, Mode::All, "ßTRAßE", None);
    assert_slices(title, &sharp[0].ranges, &["ẞtraße"]);

    let single = search(rows, Mode::All, "i", None);
    assert_slices(title, &single[0].ranges, &["İ"]);
}

#[test]
fn debug_shows_lengths_not_bodies() {
    let history = vec![text_entry(1, "history-canary")];
    let saved = vec![secret(2, "title-canary", "body-canary", "user-canary")];
    let found = search(
        Rows {
            history: &history,
            saved: &saved,
        },
        Mode::All,
        "",
        None,
    );
    let shown = format!("{found:?}");
    for canary in [
        "history-canary",
        "title-canary",
        "body-canary",
        "user-canary",
    ] {
        assert!(!shown.contains(canary), "{shown}");
    }
    assert!(shown.contains("Text(len 14)"), "{shown}");
    assert!(shown.contains("body_len"), "{shown}");
}

fn history_paths(stored: &Stored) -> Vec<PathBuf> {
    match &stored.entry.content {
        Content::Paths { paths, .. } => paths.clone(),
        Content::Text(_) => Vec::new(),
    }
}

#[test]
fn a_trailing_separator_names_the_same_folder() {
    let saved = vec![
        note(1, "here", Some("D:\\Projects\\")),
        note(2, "far", None),
    ];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let found = search(rows, Mode::Notes, "", Some(Path::new("D:\\Projects")));
    assert_eq!(ids(&found), vec![1, 2]);
    let reversed = search(rows, Mode::Notes, "", Some(Path::new("d:/projects/")));
    assert_eq!(ids(&reversed), vec![1, 2]);
}

#[test]
fn a_secret_matches_by_user_name_in_any_case_in_all_mode() {
    let saved = vec![secret(11, "login", "canary-body", "OctoCat")];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let found = search(rows, Mode::All, "OCTO", None);
    assert_eq!(found[0].field, Some(Field::User));
    assert_slices("OctoCat", &found[0].ranges, &["Octo"]);
}

#[test]
fn a_title_match_hides_a_body_match() {
    let saved = vec![saved(1, SavedKind::Snippet, "alpha", Some("alpha body"))];
    let rows = Rows {
        history: &[],
        saved: &saved,
    };
    let found = search(rows, Mode::All, "alpha", None);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].field, Some(Field::Title));
}

#[test]
fn large_non_matching_rows_are_skipped_quickly() {
    let big = "ab".repeat(128 * 1024);
    let unicode = "é".repeat(128 * 1024);
    let history: Vec<Stored> = (0..250)
        .flat_map(|n| [text_entry(n, &big), text_entry(n + 1000, &unicode)])
        .collect();
    let rows = Rows {
        history: &history,
        saved: &[],
    };
    let started = std::time::Instant::now();
    assert!(search(rows, Mode::Recent, "zzz", None).is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn ranges_per_row_are_capped() {
    let history = vec![
        text_entry(1, &"a".repeat(256 * 1024)),
        text_entry(2, &"é".repeat(10_000)),
    ];
    let rows = Rows {
        history: &history,
        saved: &[],
    };
    let ascii = search(rows, Mode::Recent, "a", None);
    assert_eq!(ascii[0].ranges.len(), MAX_RANGES);
    let accented = search(rows, Mode::Recent, "é", None);
    assert_eq!(accented[0].ranges.len(), MAX_RANGES);
}
