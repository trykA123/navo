use gpui_kit::ClipboardItem;
use rusty_structured::{Key, Syntax};

use super::{Format, TreeView};
use crate::prelude::*;

const COPY_CAP: usize = 1024 * 1024;
const CUT: &str = "Copied the first 1 MiB of this value.";
const SHORTENED: &str =
    "This value is shortened in the tree. Press 1 to view the text and copy it from there.";
const NO_KEY: &str = "The root has no key.";

fn syntax(format: Format) -> Syntax {
    match format {
        Format::Json | Format::Jsonc => Syntax::Json,
        Format::Yaml => Syntax::Yaml,
        Format::Xml => Syntax::Xml,
    }
}

impl TreeView {
    pub(super) fn copy_value(&mut self, cx: &mut Context<Self>) {
        let Some(ready) = self.ready() else {
            return;
        };
        let Some(node) = ready.outline.selected() else {
            return;
        };
        let exported = ready.tree.export(node, syntax(self.format), COPY_CAP);
        if exported.is_clipped || exported.is_partial {
            return self.set_note(SHORTENED, cx);
        }
        let note = if exported.is_cut {
            CUT
        } else {
            "Copied the value."
        };
        cx.write_to_clipboard(ClipboardItem::new_string(exported.text));
        self.set_note(note, cx);
    }

    pub(super) fn copy_key(&mut self, cx: &mut Context<Self>) {
        let Some(ready) = self.ready() else {
            return;
        };
        let Some(node) = ready.outline.selected() else {
            return;
        };
        let key = match ready.tree.key(node) {
            Key::Name(name) => name.to_owned(),
            Key::Index(index) => index.to_string(),
            Key::Root => return self.set_note(NO_KEY, cx),
        };
        cx.write_to_clipboard(ClipboardItem::new_string(key.clone()));
        self.set_note(format!("Copied {key}"), cx);
    }
}
