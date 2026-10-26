use gpui_kit::Stateful;
use rusty_l10n::t;

use super::FerriteWindow;
use crate::multi_rename::keys as mr;
use crate::multi_rename::view::{name_editor, name_tools};
use crate::prelude::*;
use crate::tokens;

impl FerriteWindow {
    pub(super) fn rename_tools(&self, cx: &App) -> Div {
        let per_folder = self
            .multi_rename
            .open
            .as_ref()
            .is_some_and(|open| open.form.counter_per_folder);
        let error = self
            .multi_rename
            .open
            .as_ref()
            .filter(|open| open.names.is_none())
            .and_then(|open| open.form.names_error(open.source.items.len()))
            .map(|error| {
                t!(
                    "rename-names-count",
                    expected = error.expected,
                    actual = error.actual
                )
            });
        let t = tokens::current(cx);
        name_tools(per_folder, cx).children(error.map(|error| {
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.mark)
                .child(error)
        }))
    }

    pub(super) fn rename_names_editor(&self, cx: &App) -> Option<Div> {
        let open = self.multi_rename.open.as_ref()?;
        let editor = open.names.as_ref()?;
        let error = open.form.names_error(open.source.items.len()).map(|error| {
            t!(
                "rename-names-count",
                expected = error.expected,
                actual = error.actual
            )
        });
        Some(name_editor(&editor.input, error, cx))
    }

    pub(super) fn rename_name_actions(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(
            cx.listener(|view, _: &mr::EditNames, window, cx| view.edit_rename_names(window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &mr::LoadNames, window, cx| view.load_rename_names(window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &mr::PasteNames, window, cx| view.paste_rename_names(window, cx)),
        )
        .on_action(cx.listener(|view, _: &mr::CopyNames, _, cx| view.copy_rename_names(cx)))
        .on_action(cx.listener(|view, _: &mr::CounterPerFolder, window, cx| {
            view.toggle_folder_counter(window, cx)
        }))
    }
}
