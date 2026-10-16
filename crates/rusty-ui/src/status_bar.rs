use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement, Styled, Window, div,
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
                format::clock(ms)
            )),
            Self::Offline { cached_at_ms: None } => Some("Offline · showing cached".to_owned()),
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
        let parts = [
            Some(self.contents_text()),
            self.mark.text(),
            Some(self.free_text()),
        ];
        self.marked_text()
            .into_iter()
            .chain(parts.into_iter().flatten())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(IntoElement)]
pub struct StatusBar {
    id: ElementId,
    props: StatusBarProps,
}

impl StatusBar {
    pub fn new(id: impl Into<ElementId>, props: StatusBarProps) -> Self {
        Self {
            id: id.into(),
            props,
        }
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let props = self.props;
        div()
            .id(self.id)
            .role(Role::Status)
            .aria_label(props.text())
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
            .when_some(props.marked_text(), |d, marked| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(c.mark)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{marked} ·")),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(props.contents_text()),
            )
            .when_some(props.mark.text(), |d, mark| {
                d.child(div().flex_none().child(mark))
            })
            .child(div().flex_none().child(props.free_text()))
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::AppContext as _;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Context, IntoElement, Render, Role, TestAppContext, Window};

    use super::{ListingMark, StatusBar, StatusBarProps};
    use crate::format;
    use crate::test_window::open;

    struct Status(StatusBarProps);

    impl Render for Status {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            StatusBar::new("status", self.0)
        }
    }

    fn label(props: StatusBarProps, cx: &mut TestAppContext) -> Option<String> {
        let (window, _) = open(cx, |_, _| Status(props));
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
            format::clock(ms)
        );
        assert_eq!(label(props, cx), Some(expected));
    }
}
