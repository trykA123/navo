use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{FocusHandle, ListOffset};
use rusty_structured::markdown::{Block, BlockKind, Document};

use super::{MarkdownFile, MarkdownView, shown};
use crate::prelude::*;
use crate::window_view::viewer::body::Host;

fn same_table(old: &Document, (was, is): (u32, u32), new: &Document) -> bool {
    let (Some(before), Some(after)) = (old.table(was), new.table(is)) else {
        return false;
    };
    before.widths() == after.widths()
        && before.alignments() == after.alignments()
        && (0..before.widths().len())
            .all(|column| old.column_word(was, column) == new.column_word(is, column))
}

fn same_block(old: &Document, was: &Block, new: &Document, is: &Block) -> bool {
    let kinds_match = match (was.kind, is.kind) {
        (
            BlockKind::Row {
                table: before,
                is_head: was_head,
            },
            BlockKind::Row {
                table: after,
                is_head,
            },
        ) => was_head == is_head && same_table(old, (before, after), new),
        (before, after) => before == after,
    };
    kinds_match
        && (was.marker, was.quote, was.list, was.is_continued)
            == (is.marker, is.quote, is.list, is.is_continued)
        && old.text(was) == new.text(is)
        && old.spans(was) == new.spans(is)
}

pub(super) fn changed_blocks(old: &Document, new: &Document) -> (Range<usize>, usize) {
    let (before, after) = (old.blocks(), new.blocks());
    let same = |(was, is): (&Block, &Block)| same_block(old, was, new, is);
    let prefix = before
        .iter()
        .zip(after)
        .take_while(|pair| same(*pair))
        .count();
    if prefix == before.len() && prefix == after.len() {
        return (prefix..prefix, 0);
    }
    let room = before.len().min(after.len()) - prefix;
    let suffix = before
        .iter()
        .rev()
        .zip(after.iter().rev())
        .take(room)
        .take_while(|pair| same(*pair))
        .count();
    let start = prefix.saturating_sub(1);
    let suffix = suffix.saturating_sub(1);
    (start..before.len() - suffix, after.len() - suffix - start)
}

impl MarkdownView {
    pub(in crate::window_view::viewer) fn preview(host: Host, cx: &mut Context<Self>) -> Self {
        let empty = MarkdownFile {
            doc: Document::default(),
            len: 0,
        };
        let mut view = Self::new(empty, host, cx);
        view.is_preview = true;
        view
    }

    pub(in crate::window_view::viewer) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(in crate::window_view::viewer) fn replace_doc(
        &mut self,
        doc: Document,
        cx: &mut Context<Self>,
    ) {
        let (old, count) = changed_blocks(&self.doc, &doc);
        let top = self.list.logical_scroll_top();
        self.list.splice(old.clone(), count);
        if old.contains(&top.item_ix) && count > 0 {
            let item_ix = top.item_ix.min(old.start + count - 1);
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: top.offset_in_item,
            });
        }
        self.doc = Rc::new(doc);
        self.picked = None;
        self.pointed = None;
        self.pointed_block = None;
        self.menu_block = None;
        cx.notify();
    }

    pub(in crate::window_view::viewer) fn reveal_line(
        &mut self,
        line: u64,
        cx: &mut Context<Self>,
    ) {
        let line = u32::try_from(line).unwrap_or(u32::MAX);
        let Some(block) = self.doc.block_at_line(line) else {
            return;
        };
        let view = self.list.viewport_bounds();
        match self.list.bounds_for_item(block) {
            Some(shown) if shown.bottom() > view.top() && shown.top() < view.bottom() => return,
            Some(_) => self.list.scroll_to_reveal_item(block),
            None => self.list.scroll_to(ListOffset {
                item_ix: block,
                offset_in_item: px(0.),
            }),
        }
        cx.notify();
    }

    pub(in crate::window_view::viewer) fn status(&self) -> Option<SharedString> {
        if let Some(note) = &self.note {
            return Some(note.clone());
        }
        let url = self.picked.and_then(|link| self.target(link))?;
        Some(format!("Ctrl+click or Enter opens {}", shown(&url)).into())
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn rendered(&self) -> String {
        let doc = &self.doc;
        let texts: Vec<&str> = doc.blocks().iter().map(|block| doc.text(block)).collect();
        texts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use rusty_structured::markdown::parse;

    use super::changed_blocks;

    fn changed(old: &str, new: &str) -> (std::ops::Range<usize>, usize) {
        let (old, new) = (parse(old).expect("old"), parse(new).expect("new"));
        changed_blocks(&old, &new)
    }

    #[test]
    fn an_edit_in_one_block_splices_it_and_its_neighbours() {
        let old = "a\n\nb\n\nc\n\nd\n\ne\n";
        assert_eq!(changed(old, "a\n\nb\n\nC\n\nd\n\ne\n"), (1..4, 3));
    }

    #[test]
    fn a_new_block_grows_the_splice() {
        let old = "a\n\nb\n\nc\n\nd\n";
        assert_eq!(changed(old, "a\n\nb\n\nx\n\nc\n\nd\n"), (1..3, 3));
    }

    #[test]
    fn an_unchanged_document_splices_nothing() {
        let old = "a\n\nb\n\nc\n";
        assert_eq!(changed(old, old), (3..3, 0));
    }

    #[test]
    fn a_wider_cell_respaces_every_row_of_its_table() {
        let old = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
        let new = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | longer |\n";
        assert_eq!(changed(old, new), (0..3, 3));
    }
}
