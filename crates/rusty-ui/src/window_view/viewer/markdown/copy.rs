use gpui_kit::ClipboardItem;

use super::{CopyBlock, CopyLink, MarkdownView, OpenLink, shown};
use crate::prelude::*;
use crate::window_view::viewer::body::MenuEntry;
use crate::window_view::viewer::platform_lines;

impl MarkdownView {
    pub(super) fn point_block(&mut self, block: usize) {
        self.pointed_block = Some(block);
    }

    fn shown_block(&self) -> Option<usize> {
        let picked = self.picked.and_then(|link| self.doc.link(link));
        match picked {
            Some(link) => Some(link.block()),
            None if self.doc.blocks().is_empty() => None,
            None => Some(self.list.logical_scroll_top().item_ix),
        }
    }

    pub(super) fn menu_entries(&mut self, by_pointer: bool) -> Vec<MenuEntry> {
        let pointed = self.pointed.take();
        let pointed_block = self.pointed_block.take();
        let (link, block) = if by_pointer {
            (pointed, pointed_block)
        } else {
            (self.picked, self.shown_block())
        };
        self.menu_block = block;
        let mut entries = Vec::new();
        if block.is_some() {
            entries.push(MenuEntry::Run("Copy", Box::new(CopyBlock)));
        }
        if link.is_some() {
            if !entries.is_empty() {
                entries.push(MenuEntry::Break);
            }
            entries.push(MenuEntry::Run("Open link", Box::new(OpenLink)));
            entries.push(MenuEntry::Run("Copy link", Box::new(CopyLink)));
        }
        entries
    }

    pub(super) fn copy_block(&mut self, cx: &mut Context<Self>) {
        let block = self.menu_block.take().or_else(|| self.shown_block());
        let Some(block) = block.and_then(|block| self.doc.blocks().get(block)) else {
            return;
        };
        let text = platform_lines(self.doc.text(block));
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.say("Copied the text of this block.", cx);
    }

    pub(super) fn copy_link(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.picked.and_then(|link| self.target(link)) else {
            return self.say("No link picked. Tab picks the next link.", cx);
        };
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        self.say(format!("Copied {}", shown(&url)), cx);
    }
}
