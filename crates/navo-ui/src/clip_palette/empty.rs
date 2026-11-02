use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, StatefulInteractiveElement, Styled, div,
};
use navo_l10n::tr;

use super::ClipMode;
use super::view::ClipPalette;
use crate::primitives::Kbd;
use crate::tokens::Theme;

pub(super) struct Say {
    title: String,
    body: String,
    pub(super) keys: Vec<(&'static str, &'static str)>,
}

fn hint(t: &Theme, id: impl Into<ElementId>, key: &str, label: &str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .children(Kbd::new(id, key))
        .child(tr(label))
}

impl ClipPalette {
    pub(super) fn says(&self, mode: ClipMode, has_query: bool) -> Say {
        if has_query {
            return Say {
                title: tr("clip-empty-search-title"),
                body: tr("clip-empty-search-body"),
                keys: vec![("escape", "clip-key-clear")],
            };
        }
        let (title, body, keys) = match mode {
            ClipMode::All | ClipMode::Recent => {
                ("clip-empty-history-title", "clip-empty-history-body", None)
            }
            // Notes has no mode of its own until its atom lands.
            ClipMode::Saved | ClipMode::Notes => (
                "clip-empty-saved-title",
                "clip-empty-saved-body",
                Some(("ctrl-s", "clip-act-save-from-history")),
            ),
            ClipMode::Secrets => ("clip-empty-secrets-title", "clip-empty-secrets-body", None),
        };
        Say {
            title: tr(title),
            body: tr(body),
            keys: keys.into_iter().collect(),
        }
    }

    /// The text that stands in for an empty list; nothing until the first answer arrives.
    pub(super) fn empty(&self, t: &Theme, cx: &App) -> AnyElement {
        let loaded = self.state.is_loaded().then(|| self.state.mode());
        self.empty_for(loaded, t, cx)
    }

    /// The same for the mode a list stands in for; blank while `loaded` is none. Esc leaves
    /// the library instead of clearing the search, so it offers no such key there.
    pub(super) fn empty_for(&self, loaded: Option<ClipMode>, t: &Theme, cx: &App) -> AnyElement {
        let Some(mode) = loaded else {
            return div().into_any_element();
        };
        let Say {
            title,
            body,
            mut keys,
        } = self.says(mode, !self.query(cx).is_empty());
        keys.retain(|(key, _)| self.manage.is_none() || *key != "escape");
        div()
            .id("clip-palette-empty")
            .role(Role::Status)
            .aria_label(format!("{title}. {body}"))
            .test_support()
            .flex_1()
            .flex()
            .flex_col()
            .items_start()
            .gap(t.spacing.xs)
            .px(t.spacing.xl)
            .py(t.spacing.xxl)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(
                div()
                    .text_size(t.typography.body)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text)
                    .child(title),
            )
            .child(div().child(body))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(t.spacing.md)
                    .gap_y(t.spacing.xs)
                    .pt(t.spacing.xs)
                    .children(keys.iter().enumerate().map(|(n, (key, label))| {
                        hint(t, ("clip-palette-empty-key", n), key, label)
                    })),
            )
            .into_any_element()
    }
}
