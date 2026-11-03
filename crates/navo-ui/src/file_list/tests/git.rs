use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Pixels, Role, TestAppContext};
use navo_fs::EntryKind;

use super::{actions, entry, open_list, props, sample_rows};
use crate::file_list::git_cell::{self, Mark};
use crate::file_list::{FileListAction, FileListProps, git_text};
use crate::git::{BadgeKind, FolderBadges, RepoBadge};
use crate::tokens::{self, ColorTheme, Mode, Theme, contrast_ratio};

fn badge(kind: BadgeKind, is_root: bool) -> RepoBadge {
    RepoBadge {
        kind,
        is_root,
        stale: false,
        branch: is_root.then(|| "main".to_owned()),
        upstream: is_root.then(|| "origin/main".to_owned()),
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: None,
        reason: None,
    }
}

fn with_git(mut shown: FileListProps, badges: &[(&str, RepoBadge)]) -> FileListProps {
    let by_name: HashMap<String, RepoBadge> = badges
        .iter()
        .map(|(name, badge)| ((*name).to_owned(), badge.clone()))
        .collect();
    shown.git = Some(Arc::new(FolderBadges {
        revision: 1,
        by_name,
        ..FolderBadges::default()
    }));
    shown
}

fn one_repo() -> FileListProps {
    let modified = RepoBadge {
        changed: 2,
        ..badge(BadgeKind::Modified, true)
    };
    with_git(props(sample_rows()), &[("src", modified)])
}

fn name_left(cx: &mut TestAppContext, shown: FileListProps, row: usize) -> Pixels {
    let (window, _) = open_list(cx, shown);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        list.within(("row", row)).find("find-name").bounds().left()
    })
    .expect("window")
}

fn has(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize, id: &'static str) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        list.within(("row", row)).try_find(id).is_some()
    })
    .expect("window")
}

#[gpui_kit::test]
fn no_git_header_is_drawn_and_only_the_badged_row_has_a_mark(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, one_repo());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("left").try_find("git-header").is_none());
    })
    .expect("window");
    assert!(has(cx, window, 0, "git-badge"));
    assert!(!has(cx, window, 1, "git-badge"));
    assert!(!has(cx, window, 2, "git-badge"));
}

#[gpui_kit::test]
fn names_start_at_the_same_x_with_or_without_badges(cx: &mut TestAppContext) {
    let plain = name_left(cx, props(sample_rows()), 0);
    assert_eq!(name_left(cx, one_repo(), 0), plain);
    assert_eq!(name_left(cx, one_repo(), 1), plain);
    let empty = with_git(props(sample_rows()), &[]);
    assert_eq!(name_left(cx, empty, 0), plain);
}

const EVERY_KIND: [BadgeKind; 8] = [
    BadgeKind::Clean,
    BadgeKind::Modified,
    BadgeKind::Ahead,
    BadgeKind::Behind,
    BadgeKind::Diverged,
    BadgeKind::Conflict,
    BadgeKind::Unknown,
    BadgeKind::Checking,
];

#[gpui_kit::test]
fn every_state_draws_its_mark_on_the_corner_of_the_folder_icon(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..EVERY_KIND.len()).map(|n| format!("repo{n}")).collect();
    let rows: Arc<[_]> = names
        .iter()
        .map(|name| entry(name, EntryKind::Folder, 0))
        .collect();
    let badges: Vec<(&str, RepoBadge)> = names
        .iter()
        .zip(EVERY_KIND)
        .map(|(name, kind)| (name.as_str(), badge(kind, true)))
        .collect();
    let (window, _) = open_list(cx, with_git(props(rows), &badges));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        for row in 0..EVERY_KIND.len() {
            let row = list.within(("row", row));
            let icon = row.find("row-icon").bounds();
            let mark = row.find("git-badge").bounds();
            assert!(mark.right() > icon.right() && mark.bottom() > icon.bottom());
            assert!(mark.left() < icon.right() && mark.top() < icon.bottom());
        }
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_badge_is_a_button_with_a_spoken_name(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, one_repo());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        let badge = list.within(("row", 0usize)).find("git-badge");
        assert_eq!(badge.role(), Some(Role::Button));
        assert_eq!(badge.label(), Some("Git: Modified"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_click_on_the_badge_reports_its_row_and_does_not_open_it(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, one_repo());
    cx.update_window(window, |_, window, cx| {
        let mut list = window.within("left");
        list.within(("row", 0usize)).click("git-badge", cx);
    })
    .expect("window");
    cx.run_until_parked();
    let seen = actions(&harness, cx);
    assert!(matches!(seen[..], [FileListAction::GitMenu { row: 0, .. }]));
}

#[gpui_kit::test]
fn files_never_get_a_badge_even_with_a_matching_name(cx: &mut TestAppContext) {
    let rows: Arc<[_]> = vec![
        entry("src", EntryKind::File, 10),
        entry("lib", EntryKind::Folder, 0),
    ]
    .into();
    let lib = badge(BadgeKind::Clean, true);
    let shown = with_git(props(rows), &[("src", lib.clone()), ("lib", lib)]);
    let (window, _) = open_list(cx, shown);
    assert!(!has(cx, window, 0, "git-badge"));
    assert!(has(cx, window, 1, "git-badge"));
}

#[test]
fn spoken_names_give_state_and_non_zero_counts() {
    let both = RepoBadge {
        ahead: 2,
        behind: 1,
        ..badge(BadgeKind::Diverged, true)
    };
    assert_eq!(git_text::spoken(&both), "Git: Diverged, 2 ahead, 1 behind");
    let ahead = RepoBadge {
        ahead: 3,
        ..badge(BadgeKind::Ahead, true)
    };
    assert_eq!(git_text::spoken(&ahead), "Git: Ahead, 3 ahead");
    assert_eq!(
        git_text::spoken(&badge(BadgeKind::Clean, true)),
        "Git: Clean"
    );
    assert_eq!(
        git_text::spoken(&badge(BadgeKind::Checking, true)),
        "Git: Checking"
    );
    assert_eq!(
        git_text::spoken(&badge(BadgeKind::Unknown, false)),
        "Git: Unknown"
    );
}

#[test]
fn a_modified_repository_still_speaks_its_ahead_and_behind_counts() {
    let modified = RepoBadge {
        changed: 1,
        ahead: 1,
        behind: 4,
        ..badge(BadgeKind::Modified, true)
    };
    assert_eq!(
        git_text::spoken(&modified),
        "Git: Modified, 1 ahead, 4 behind"
    );
    assert_eq!(git_text::counts(&modified).as_deref(), Some("1↑ 4↓"));
}

#[test]
fn counts_hide_zeros_and_belong_to_roots_only() {
    let ahead = RepoBadge {
        ahead: 5,
        ..badge(BadgeKind::Ahead, true)
    };
    assert_eq!(git_text::counts(&ahead).as_deref(), Some("5↑"));
    let behind = RepoBadge {
        behind: 2,
        ..badge(BadgeKind::Behind, true)
    };
    assert_eq!(git_text::counts(&behind).as_deref(), Some("2↓"));
    assert_eq!(git_text::counts(&badge(BadgeKind::Clean, true)), None);
    let inner = RepoBadge {
        ahead: 5,
        ..badge(BadgeKind::Clean, false)
    };
    assert_eq!(git_text::counts(&inner), None);
}

#[test]
fn the_tooltip_names_state_branch_counts_and_fetch_time() {
    let modified = RepoBadge {
        changed: 3,
        untracked: 1,
        ahead: 2,
        behind: 0,
        fetched_ms: Some(1_791_920_400_000),
        ..badge(BadgeKind::Modified, true)
    };
    let tip = git_text::tooltip(&modified);
    let lines: Vec<&str> = tip.lines().collect();
    assert_eq!(lines[0], "main → origin/main");
    assert_eq!(lines[1], "Modified: 4 changed, 1 new");
    assert!(
        lines[2].starts_with("2 ahead · 0 behind · fetched "),
        "{tip}"
    );
    let both = RepoBadge {
        ahead: 1,
        behind: 2,
        ..badge(BadgeKind::Diverged, true)
    };
    assert!(git_text::tooltip(&both).contains("1 ahead · 2 behind"));
    let unfetched = RepoBadge {
        fetched_ms: None,
        ..badge(BadgeKind::Clean, true)
    };
    assert!(git_text::tooltip(&unfetched).ends_with("not fetched yet"));
}

#[test]
fn a_subfolder_tooltip_says_it_is_inside_a_repository_and_gives_no_count() {
    let inner = badge(BadgeKind::Modified, false);
    assert_eq!(git_text::tooltip(&inner), "Inside a repository\nModified");
}

#[test]
fn a_clean_root_shows_the_check_and_a_clean_subfolder_the_quiet_dot() {
    let t = Theme::new(Mode::Light);
    assert!(matches!(
        git_cell::mark(&t, &badge(BadgeKind::Clean, true)),
        Mark::Glyph(..)
    ));
    assert_eq!(
        git_cell::mark(&t, &badge(BadgeKind::Clean, false)),
        Mark::Dot(t.git.quiet)
    );
}

#[test]
fn every_glyph_state_has_its_own_shape() {
    let t = Theme::new(Mode::Dark);
    let shapes: Vec<_> = [
        BadgeKind::Conflict,
        BadgeKind::Modified,
        BadgeKind::Ahead,
        BadgeKind::Behind,
        BadgeKind::Diverged,
        BadgeKind::Unknown,
        BadgeKind::Checking,
    ]
    .map(|kind| match git_cell::mark(&t, &badge(kind, true)) {
        Mark::Glyph(name, _) => format!("{name:?}"),
        Mark::Dot(_) => String::new(),
    })
    .into();
    let mut unique = shapes.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), shapes.len(), "{shapes:?}");
    assert!(!shapes.iter().any(String::is_empty));
}

fn every_theme() -> impl Iterator<Item = (ColorTheme, Mode, tokens::Theme)> {
    ColorTheme::ALL.into_iter().flat_map(|color_theme| {
        [Mode::Light, Mode::Dark].map(|mode| {
            let look = tokens::ThemeLook {
                color_theme,
                ..tokens::ThemeLook::default()
            };
            (color_theme, mode, tokens::Theme::with_look(mode, look))
        })
    })
}

#[test]
fn state_colours_stay_three_to_one_on_the_row_and_the_selected_row() {
    for (color_theme, mode, t) in every_theme() {
        for (ground, row) in [("row", t.surfaces.card), ("selected", t.surfaces.current)] {
            for (name, color) in [
                ("conflict", t.git.conflict),
                ("modified", t.git.modified),
                ("sync", t.git.sync),
                ("quiet", t.git.quiet),
                ("ok", t.colors.ok),
            ] {
                let ratio = contrast_ratio(color, row);
                assert!(
                    ratio >= 3.,
                    "{color_theme:?} {mode:?} {name} on {ground}: {ratio}"
                );
            }
        }
    }
}
