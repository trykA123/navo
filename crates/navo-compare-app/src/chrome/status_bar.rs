//! Presentational status bar (NC66): muted summary left and non-focusable `Kbd` hints right.

use std::rc::Rc;

use gpui_kit::{
    RenderOnce, Role,
    assets::IconName,
    base::{TestSupportExt as _, TestTextExt as _},
    div,
};
use sidewise_kit::{
    prelude::*,
    primitives::{Kbd, icon},
    tokens,
};
use sidewise_l10n::t;

/// One keyboard hint displayed on the right side of [`StatusBar`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusHint {
    pub id: ElementId,
    pub keys: SharedString,
    pub label: SharedString,
}

/// An update notice: its text and the two clicks, both run by the caller.
#[derive(Clone)]
pub struct StatusNotice {
    pub text: SharedString,
    pub on_open: Rc<dyn Fn(&mut App)>,
    pub on_hide: Rc<dyn Fn(&mut App)>,
}

/// Presentational status bar showing a truncated summary left and valid key hints right.
#[derive(IntoElement)]
pub struct StatusBar {
    summary: SharedString,
    hints: Vec<StatusHint>,
    notice: Option<StatusNotice>,
}

impl StatusBar {
    /// Creates a presentational status bar from caller-owned summary text and key hints.
    pub fn new(
        summary: impl Into<SharedString>,
        hints: impl IntoIterator<Item = StatusHint>,
    ) -> Self {
        Self {
            summary: summary.into(),
            hints: hints.into_iter().collect(),
            notice: None,
        }
    }

    /// Adds (or clears) the update notice drawn between the summary and the hints.
    pub fn notice(mut self, notice: Option<StatusNotice>) -> Self {
        self.notice = notice;
        self
    }
}

/// Builds one non-focusable `Kbd` + label hint row, omitting invalid key strings.
fn hint_view(hint: StatusHint, gap: Pixels) -> Option<Div> {
    let kbd = Kbd::new(hint.id, &hint.keys)?;
    Some(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(gap)
            .child(kbd)
            .child(hint.label.clone().test_text(hint.label)),
    )
}

/// Builds the notice row: accent dot, clickable text and a hide cross; neither takes focus.
fn notice_view(notice: StatusNotice, t: &tokens::Theme) -> impl IntoElement {
    let (on_open, on_hide) = (notice.on_open, notice.on_hide);
    let label = SharedString::from(t!("update-hide"));
    div()
        .id("status-update")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .child(div().size(px(6.)).rounded(t.radii.full).bg(t.colors.accent))
        .child(
            div()
                .id("status-update-open")
                .test_support()
                .cursor_pointer()
                .text_color(t.colors.text)
                .on_click(move |_, _, cx| on_open(cx))
                .child(notice.text.clone().test_text(notice.text)),
        )
        .child(
            div()
                .id("status-update-hide")
                .role(Role::Button)
                .aria_label(label)
                .test_support()
                .cursor_pointer()
                .on_click(move |_, _, cx| on_hide(cx))
                .child(icon(IconName::Close, px(12.), t.colors.muted)),
        )
}

impl RenderOnce for StatusBar {
    /// Draws the footer on matte surface (`rail`) with a hairline (`line`), keeping full summary text.
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let summary = div()
            .id("status-summary")
            .role(Role::Label)
            .aria_label(self.summary.clone())
            .test_support()
            .flex_1()
            .min_w_0()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(self.summary.clone().test_text(self.summary));
        let hints = self
            .hints
            .into_iter()
            .filter_map(|h| hint_view(h, t.spacing.sm));
        let keys = div()
            .id("status-hints")
            .test_support()
            .flex()
            .min_w_0()
            .items_center()
            .gap(t.spacing.lg)
            .overflow_hidden()
            .children(hints);
        div()
            .id("status-bar")
            .role(Role::Status)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.lg)
            .w_full()
            .overflow_hidden()
            .h(t.spacing.status_height)
            .px(t.spacing.xxl)
            .bg(t.colors.rail)
            .border_t_1()
            .border_color(t.colors.line)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(summary)
            .children(self.notice.map(|n| notice_view(n, t)))
            .child(keys)
    }
}
