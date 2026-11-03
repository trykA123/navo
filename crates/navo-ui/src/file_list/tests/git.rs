use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Context, IntoElement, ParentElement, Pixels, Render, Role, Styled,
    TestAppContext, Window, div, px,
};
use navo_fs::EntryKind;

use super::{actions, entry, open_list, props, sample_rows};
use crate::file_list::git_cell::{self, Mark};
use crate::file_list::{FileList, FileListAction, FileListProps, git_text};
use crate::git::{BadgeKind, FolderBadges, RepoBadge};
use crate::test_window::open_sized;
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

struct Sized(FileListProps);

impl Render for Sized {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .h(px(480.))
                .flex()
                .flex_col()
                .child(FileList::new("left", self.0.clone())),
        )
    }
}

fn gutter_probe(
    cx: &mut TestAppContext,
    width: f32,
    shown: FileListProps,
) -> (Pixels, Pixels, Pixels, Pixels) {
    let (window, _) = open_sized(cx, px(width), move |_, _| Sized(shown));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        let header = list.find("git-header").bounds();
        let badge = list.within(("row", 0usize)).find("git-badge").bounds();
        (
            header.left(),
            header.size.width,
            badge.left(),
            badge.size.width,
        )
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
fn the_gutter_is_absent_when_no_row_has_a_badge(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("left").try_find("git-header").is_none());
    })
    .expect("window");
    assert!(!has(cx, window, 0, "git-badge"));
}

#[gpui_kit::test]
fn the_gutter_is_absent_when_the_badge_list_is_empty(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, with_git(props(sample_rows()), &[]));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("left").try_find("git-header").is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_gutter_shows_one_badge_and_leaves_other_rows_empty(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, one_repo());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within("left");
        assert!(list.find("git-header").visible());
        assert_eq!(list.find("git-header").label(), Some("Git"));
    })
    .expect("window");
    assert!(has(cx, window, 0, "git-badge"));
    assert!(!has(cx, window, 1, "git-badge"));
    assert!(!has(cx, window, 2, "git-badge"));
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
fn header_and_row_gutters_line_up_at_every_width(cx: &mut TestAppContext) {
    for width in [480., 800., 1280.] {
        let (header_left, header_width, badge_left, badge_width) =
            gutter_probe(cx, width, one_repo());
        assert_eq!(header_left, badge_left, "{width}");
        assert_eq!(header_width, badge_width, "{width}");
        assert!(header_width > px(0.), "{width}");
    }
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
