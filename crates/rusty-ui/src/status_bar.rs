use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div,
};

use rusty_l10n::t;

use crate::format;
use crate::tokens;

mod chips;
mod workspace;

use chips::{marked_chip, marked_part, notice_text};
pub use workspace::{WorkspaceButton, WorkspaceSummary};

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
    pub hidden_marks: u64,
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

    fn hidden_text(&self) -> Option<String> {
        (self.hidden_marks > 0).then(|| t!("files-marks-hidden", count = self.hidden_marks))
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
            .into_iter()
            .chain(self.hidden_text())
            .filter(|_| parts.marked_totals)
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
    workspace: Option<WorkspaceButton>,
    summary: Option<SharedString>,
}

impl StatusBar {
    pub fn new(id: impl Into<ElementId>, props: StatusBarProps) -> Self {
        Self {
            id: id.into(),
            props,
            notice: None,
            parts: FooterParts::default(),
            workspace: None,
            summary: None,
        }
    }

    #[must_use]
    pub(crate) fn summary(mut self, summary: Option<SharedString>) -> Self {
        if summary.is_some() {
            self.props = StatusBarProps::default();
            self.parts = FooterParts {
                contents: true,
                free_space: false,
                marked_totals: false,
            };
        }
        self.summary = summary;
        self
    }

    #[must_use]
    pub fn parts(mut self, parts: FooterParts) -> Self {
        self.parts = parts;
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.summary.is_none() && self.props.text_in(self.parts).is_empty() && self.notice.is_none()
    }

    #[must_use]
    pub fn notice(mut self, notice: Option<StatusNotice>) -> Self {
        self.notice = notice;
        self
    }

    #[must_use]
    pub fn workspace(mut self, workspace: Option<WorkspaceButton>) -> Self {
        self.workspace = workspace;
        self
    }

    fn text(&self) -> (String, String) {
        let text = self
            .summary
            .as_ref()
            .map_or_else(|| self.props.text_in(self.parts), ToString::to_string);
        let contents = self
            .summary
            .as_ref()
            .map_or_else(|| self.props.contents_text(), |text| text.to_string());
        let label = match &self.notice {
            Some(notice) if text.is_empty() => notice.text.to_string(),
            Some(notice) => format!("{} · {}", notice.text, text),
            None => text,
        };
        (label, contents)
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let props = self.props;
        let parts = self.parts;
        let (label, contents) = self.text();
        let more = parts.contents || props.mark.text().is_some() || parts.free_space;
        let hidden = props.hidden_text().filter(|_| parts.marked_totals);
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
                |d, marked| d.child(marked_chip(c.mark, marked, more || hidden.is_some())),
            )
            .when_some(hidden, |d, hidden| {
                d.child(div().flex_none().child(marked_part(hidden, more)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(parts.contents, |d| d.child(contents)),
            )
            .when_some(props.mark.text(), |d, mark| {
                d.child(div().flex_none().child(mark))
            })
            .when(parts.free_space, |d| {
                d.child(div().flex_none().child(props.free_text()))
            })
            .children(self.workspace)
    }
}

#[cfg(test)]
mod tests;
