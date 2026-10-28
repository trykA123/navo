use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Context, IntoElement, Render, Role, TestAppContext, Window};

use super::chips::marked_part;
use super::{
    FooterParts, ListingMark, StatusBar, StatusBarProps, StatusNotice, WorkspaceButton,
    WorkspaceSummary,
};
use crate::format;
use crate::test_window::open;

struct Status(StatusBarProps, Option<StatusNotice>, FooterParts);

impl Render for Status {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        StatusBar::new("status", self.0)
            .notice(self.1.clone())
            .parts(self.2)
    }
}

fn label(props: StatusBarProps, cx: &mut TestAppContext) -> Option<String> {
    label_with(props, None, cx)
}

fn label_with(
    props: StatusBarProps,
    notice: Option<StatusNotice>,
    cx: &mut TestAppContext,
) -> Option<String> {
    label_in(props, notice, FooterParts::default(), cx)
}

fn label_in(
    props: StatusBarProps,
    notice: Option<StatusNotice>,
    parts: FooterParts,
    cx: &mut TestAppContext,
) -> Option<String> {
    let (window, _) = open(cx, move |_, _| Status(props, notice, parts));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let status = window.find("status");
        assert_eq!(status.role(), Some(Role::Status));
        status.label().map(str::to_owned)
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_status_shows_counts_sizes_and_free_space(cx: &mut TestAppContext) {
    let props = StatusBarProps {
        files: 5,
        folders: 1,
        bytes: 37_412,
        marked: 2,
        marked_bytes: 15_148,
        hidden_marks: 0,
        free_bytes: Some(512_000_000),
        mark: ListingMark::Fresh,
    };
    assert_eq!(
        label(props, cx).as_deref(),
        Some("2 marked, 14.8 KB · 5 files, 1 folder · 36.5 KB · 488.3 MB free")
    );
}

#[gpui_kit::test]
fn no_hidden_marks_leave_the_status_without_a_count(cx: &mut TestAppContext) {
    let props = busy_props(ListingMark::Fresh);
    let text = label(props, cx).expect("label");
    assert!(!text.contains("not shown"), "{text}");
}

#[gpui_kit::test]
fn hidden_marks_show_a_count_after_the_marked_totals(cx: &mut TestAppContext) {
    let props = StatusBarProps {
        hidden_marks: 3,
        ..busy_props(ListingMark::Fresh)
    };
    assert_eq!(
        label(props, cx).as_deref(),
        Some(
            "2 marked, 14.8 KB · 3 marked not shown · 5 files, 1 folder · 36.5 KB · 488.3 MB free"
        )
    );
    let one = StatusBarProps {
        hidden_marks: 1,
        ..props
    };
    assert!(
        label(one, cx)
            .expect("label")
            .contains("· 1 marked not shown ·")
    );
}

#[gpui_kit::test]
fn unknown_free_space_shows_a_dash(cx: &mut TestAppContext) {
    let props = StatusBarProps {
        files: 1,
        folders: 0,
        bytes: 812,
        ..StatusBarProps::default()
    };
    assert_eq!(
        label(props, cx).as_deref(),
        Some("1 file, 0 folders · 812 B · – free")
    );
}

#[gpui_kit::test]
fn a_cached_listing_says_so_in_the_status(cx: &mut TestAppContext) {
    let props = StatusBarProps {
        files: 2,
        mark: ListingMark::Cached,
        ..StatusBarProps::default()
    };
    assert_eq!(
        label(props, cx).as_deref(),
        Some("2 files, 0 folders · 0 B · cached · – free")
    );
}

#[gpui_kit::test]
fn an_offline_listing_says_when_its_rows_were_read(cx: &mut TestAppContext) {
    let ms = 1_791_920_400_000;
    let props = StatusBarProps {
        files: 2,
        mark: ListingMark::Offline {
            cached_at_ms: Some(ms),
        },
        ..StatusBarProps::default()
    };
    let expected = format!(
        "2 files, 0 folders · 0 B · Offline · showing cached from {} · – free",
        format::date(ms)
    );
    assert_eq!(label(props, cx), Some(expected));
}

#[gpui_kit::test]
fn a_notice_leads_the_status(cx: &mut TestAppContext) {
    let props = StatusBarProps {
        files: 1,
        ..StatusBarProps::default()
    };
    let notice = StatusNotice {
        text: "keymap.toml, line 3: unknown command `x`".into(),
        is_error: true,
    };
    assert_eq!(
        label_with(props, Some(notice), cx).as_deref(),
        Some("keymap.toml, line 3: unknown command `x` · 1 file, 0 folders · 0 B · – free")
    );
}

fn none() -> FooterParts {
    FooterParts {
        free_space: false,
        marked_totals: false,
        contents: false,
    }
}

fn busy_props(mark: ListingMark) -> StatusBarProps {
    StatusBarProps {
        files: 5,
        folders: 1,
        bytes: 37_412,
        marked: 2,
        marked_bytes: 15_148,
        hidden_marks: 0,
        free_bytes: Some(512_000_000),
        mark,
    }
}

#[test]
fn each_footer_part_leaves_the_text_when_hidden() {
    let props = busy_props(ListingMark::Fresh);
    let all = FooterParts::default();
    let text = |parts| props.text_in(parts);
    assert!(
        text(FooterParts {
            free_space: false,
            ..all
        })
        .ends_with("36.5 KB")
    );
    assert!(
        !text(FooterParts {
            marked_totals: false,
            ..all
        })
        .contains("marked")
    );
    assert!(
        !text(FooterParts {
            contents: false,
            ..all
        })
        .contains("files")
    );
    assert_eq!(text(none()), "");
}

#[test]
fn a_footer_with_every_part_hidden_is_empty_unless_it_has_a_problem() {
    let quiet = StatusBar::new("status", busy_props(ListingMark::Fresh)).parts(none());
    assert!(quiet.is_empty());
    let offline = StatusBar::new(
        "status",
        busy_props(ListingMark::Offline { cached_at_ms: None }),
    )
    .parts(none());
    assert!(!offline.is_empty());
    assert_eq!(
        offline.props.text_in(none()),
        "Offline \u{b7} showing cached"
    );
    let notice = StatusNotice {
        text: "disk full".into(),
        is_error: true,
    };
    assert!(!quiet.notice(Some(notice)).is_empty());
}

#[gpui_kit::test]
fn a_notice_alone_has_no_dangling_separator(cx: &mut TestAppContext) {
    let notice = StatusNotice {
        text: "disk full".into(),
        is_error: true,
    };
    let props = busy_props(ListingMark::Fresh);
    assert_eq!(
        label_in(props, Some(notice), none(), cx).as_deref(),
        Some("disk full")
    );
}

#[test]
fn marked_totals_alone_end_without_a_separator() {
    assert_eq!(marked_part("2 marked".into(), false), "2 marked");
    assert_eq!(marked_part("2 marked".into(), true), "2 marked ·");
}

struct WithWorkspace(WorkspaceSummary);

impl Render for WithWorkspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        StatusBar::new("status", busy_props(ListingMark::Fresh))
            .workspace(Some(WorkspaceButton::new(self.0.clone())))
    }
}

fn summary(draft: u16) -> WorkspaceSummary {
    WorkspaceSummary {
        name: "Reports".into(),
        n: 2,
        total: 4,
        draft,
    }
}

fn with_workspace<R>(
    draft: u16,
    cx: &mut TestAppContext,
    check: impl FnOnce(&mut Window) -> R,
) -> R {
    let (window, _) = open(cx, move |_, _| WithWorkspace(summary(draft)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        check(window)
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_workspace_button_ends_the_status_with_its_name_and_count(cx: &mut TestAppContext) {
    with_workspace(0, cx, |window| {
        let status = window.find("status").bounds();
        let button = window.find("workspace-button").bounds();
        assert!(button.right() <= status.right());
        assert!(button.left() > status.center().x);
        let name = window.find("workspace-button-name");
        assert_eq!(name.label(), Some("Reports"));
        assert_eq!(window.find("workspace-button-count").label(), Some("2/4"));
        assert!(window.try_find("workspace-button-draft").is_none());
    });
}

#[gpui_kit::test]
fn a_draft_shows_its_change_count_in_the_button(cx: &mut TestAppContext) {
    let expected = rusty_l10n::t!("workspace-draft", count = 3u16);
    with_workspace(3, cx, |window| {
        assert_eq!(
            window.find("workspace-button-draft").label(),
            Some(expected.as_str())
        );
    });
}

#[gpui_kit::test]
fn the_button_icon_and_labels_share_one_centre_line(cx: &mut TestAppContext) {
    with_workspace(3, cx, |window| {
        let icon = window.find("workspace-button-icon").bounds();
        assert!(icon.size.height > gpui_kit::px(0.));
        let centre = icon.center().y;
        for id in [
            "workspace-button-name",
            "workspace-button-count",
            "workspace-button-draft",
        ] {
            let offset = f32::from(window.find(id).bounds().center().y - centre).abs();
            assert!(offset <= 1., "{id} is {offset} px off the icon centre");
        }
    });
}

#[test]
fn the_workspace_button_alone_does_not_keep_a_hidden_footer() {
    let bar = StatusBar::new("status", busy_props(ListingMark::Fresh))
        .parts(none())
        .workspace(Some(WorkspaceButton::new(summary(0))));
    assert!(bar.is_empty());
}
