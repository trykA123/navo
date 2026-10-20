use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div,
};

use crate::format;
use crate::tokens;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListingMark {
    #[default]
    Fresh,
    Cached,
    Offline {
        cached_at_ms: Option<i64>,
    },
}

impl ListingMark {
    fn text(self) -> Option<String> {
        match self {
            Self::Fresh => None,
            Self::Cached => Some("cached".to_owned()),
            Self::Offline {
                cached_at_ms: Some(ms),
            } => Some(format!(
                "Offline · showing cached from {}",
                format::read_time(ms)
            )),
            Self::Offline { cached_at_ms: None } => Some("Offline · showing cached".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FooterParts {
    pub free_space: bool,
    pub marked_totals: bool,
    pub contents: bool,
}

impl Default for FooterParts {
    fn default() -> Self {
        Self {
            free_space: true,
            marked_totals: true,
            contents: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusBarProps {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    pub marked: u64,
    pub marked_bytes: u64,
    pub free_bytes: Option<u64>,
    pub mark: ListingMark,
}

impl StatusBarProps {
    fn marked_text(&self) -> Option<String> {
        (self.marked > 0).then(|| {
            format!(
                "{} marked, {}",
                format::bytes(self.marked),
                format::size(self.marked_bytes)
            )
        })
    }

    fn contents_text(&self) -> String {
        format!(
            "{}, {} · {}",
            format::count(self.files, "file", "files"),
            format::count(self.folders, "folder", "folders"),
            format::size(self.bytes)
        )
    }

    fn free_text(&self) -> String {
        match self.free_bytes {
            Some(free) => format!("{} free", format::size(free)),
            None => "– free".to_owned(),
        }
    }

    pub fn text(&self) -> String {
        self.text_in(FooterParts::default())
    }

    pub fn text_in(&self, parts: FooterParts) -> String {
        let shown = [
            parts.contents.then(|| self.contents_text()),
            self.mark.text(),
            parts.free_space.then(|| self.free_text()),
        ];
        self.marked_text()
            .filter(|_| parts.marked_totals)
            .into_iter()
            .chain(shown.into_iter().flatten())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusNotice {
    pub text: SharedString,
    pub is_error: bool,
}

#[derive(IntoElement)]
pub struct StatusBar {
    id: ElementId,
    props: StatusBarProps,
    notice: Option<StatusNotice>,
    parts: FooterParts,
}

impl StatusBar {
    pub fn new(id: impl Into<ElementId>, props: StatusBarProps) -> Self {
        Self {
            id: id.into(),
            props,
            notice: None,
            parts: FooterParts::default(),
        }
    }

    #[must_use]
    pub fn parts(mut self, parts: FooterParts) -> Self {
        self.parts = parts;
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.props.text_in(self.parts).is_empty() && self.notice.is_none()
    }

    #[must_use]
    pub fn notice(mut self, notice: Option<StatusNotice>) -> Self {
        self.notice = notice;
        self
    }
}

fn marked_part(marked: String, more: bool) -> String {
    if more { format!("{marked} ·") } else { marked }
}

fn notice_text(t: &tokens::Theme, notice: StatusNotice) -> impl IntoElement {
    let color = if notice.is_error {
        t.colors.mark
    } else {
        t.colors.warn
    };
    div()
        .flex_shrink(1.)
        .min_w_0()
        .truncate()
        .text_color(color)
        .child(notice.text)
}

impl RenderOnce for StatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let props = self.props;
        let parts = self.parts;
        let text = props.text_in(parts);
        let label = match &self.notice {
            Some(notice) if text.is_empty() => notice.text.to_string(),
            Some(notice) => format!("{} · {}", notice.text, text),
            None => text,
        };
        let more = parts.contents || props.mark.text().is_some() || parts.free_space;
        div()
            .id(self.id)
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .flex_none()
            .h(t.spacing.status_height)
            .px(t.spacing.xl)
            .text_size(t.typography.small)
            .text_color(c.muted)
            .border_t_1()
            .border_color(t.surfaces.card_edge)
            .when_some(self.notice, |d, notice| d.child(notice_text(t, notice)))
            .when_some(
                props.marked_text().filter(|_| parts.marked_totals),
                |d, marked| {
                    d.child(
                        div()
                            .flex_none()
                            .text_color(c.mark)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(marked_part(marked, more)),
                    )
                },
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(parts.contents, |d| d.child(props.contents_text())),
            )
            .when_some(props.mark.text(), |d, mark| {
                d.child(div().flex_none().child(mark))
            })
            .when(parts.free_space, |d| {
                d.child(div().flex_none().child(props.free_text()))
            })
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::AppContext as _;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Context, IntoElement, Render, Role, TestAppContext, Window};

    use super::{FooterParts, ListingMark, StatusBar, StatusBarProps, StatusNotice, marked_part};
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
            free_bytes: Some(512_000_000),
            mark: ListingMark::Fresh,
        };
        assert_eq!(
            label(props, cx).as_deref(),
            Some("2 marked, 14.8 KB · 5 files, 1 folder · 36.5 KB · 488.3 MB free")
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
}
