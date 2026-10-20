use std::collections::HashSet;
use std::hash::Hash;

use pdfium_render::prelude::PdfDocument;

use super::PdfiumEngine;
use super::links::action_target;
use crate::protocol::{
    LinkTarget, MAX_OUTLINE_ITEMS, MAX_OUTLINE_LEVEL, MAX_TITLE_CHARS, Outline, OutlineItem,
    PdfError,
};

pub(super) fn extract(
    engine: &PdfiumEngine,
    document: &PdfDocument<'_>,
) -> Result<Outline, PdfError> {
    let bookmarks = document.bookmarks();
    let items = flatten(bookmarks.root(), |bookmark| {
        let title = bookmark.title();
        let target = if title.is_some() {
            action_target(engine, bookmark.action(), || bookmark.destination())
        } else {
            LinkTarget::Unsupported
        };
        Bookmark {
            title,
            target,
            child: bookmark.first_child(),
            sibling: bookmark.next_sibling(),
        }
    });
    Outline::new(items).map_err(|_| PdfError::Corrupt)
}

struct Bookmark<T> {
    title: Option<String>,
    target: LinkTarget,
    child: Option<T>,
    sibling: Option<T>,
}

fn flatten<T: Clone + Eq + Hash>(
    first: Option<T>,
    mut read: impl FnMut(&T) -> Bookmark<T>,
) -> Vec<OutlineItem> {
    let mut pending: Vec<_> = first.into_iter().map(|bookmark| (bookmark, 0)).collect();
    let mut visited = HashSet::new();
    let mut items = Vec::new();
    while let Some((bookmark, level)) = pending.pop() {
        if items.len() == MAX_OUTLINE_ITEMS {
            break;
        }
        if !visited.insert(bookmark.clone()) {
            continue;
        }
        let bookmark = read(&bookmark);
        if let Some(sibling) = bookmark.sibling {
            pending.push((sibling, level));
        }
        if let Some(child) = bookmark.child {
            pending.push((child, (level + 1).min(MAX_OUTLINE_LEVEL)));
        }
        let Some(mut title) = bookmark.title else {
            continue;
        };
        if let Some((end, _)) = title.char_indices().nth(MAX_TITLE_CHARS) {
            title.truncate(end);
        }
        items.push(OutlineItem {
            title,
            level,
            target: bookmark.target,
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(index: usize, child: Option<usize>, sibling: Option<usize>) -> Bookmark<usize> {
        Bookmark {
            title: Some(index.to_string()),
            target: LinkTarget::Unsupported,
            child,
            sibling,
        }
    }

    #[test]
    fn an_empty_bookmark_tree_has_an_empty_outline() {
        assert!(flatten(None::<usize>, |_| panic!("no bookmark to read")).is_empty());
    }

    #[test]
    fn bookmarks_flatten_depth_first_with_their_levels_and_targets() {
        let tree = [
            (Some(1), Some(4)),
            (Some(2), Some(3)),
            (None, None),
            (None, None),
            (None, None),
        ];
        let items = flatten(Some(0), |&index| {
            let (child, sibling) = tree[index];
            let mut node = bookmark(index, child, sibling);
            node.title = Some("Same title".into());
            node.target = LinkTarget::Page {
                page: index as u32,
                top: Some(20.0),
            };
            node
        });
        assert_eq!(
            items.iter().map(|item| item.level).collect::<Vec<_>>(),
            [0, 1, 2, 1, 0]
        );
        for (index, item) in items.iter().enumerate() {
            assert_eq!(item.title, "Same title");
            assert_eq!(
                item.target,
                LinkTarget::Page {
                    page: index as u32,
                    top: Some(20.0)
                }
            );
        }
    }

    #[test]
    fn unreadable_bookmarks_are_skipped_but_their_children_and_siblings_remain() {
        let items = flatten(Some(0), |&index| {
            let mut node = bookmark(index, (index == 0).then_some(1), (index == 0).then_some(2));
            if index == 0 {
                node.title = None;
            }
            node
        });
        assert_eq!(
            items
                .iter()
                .map(|item| (item.title.as_str(), item.level))
                .collect::<Vec<_>>(),
            [("1", 1), ("2", 0)]
        );
    }

    #[test]
    fn deep_bookmarks_are_clamped_and_never_dropped() {
        let depth = 300;
        let items = flatten(Some(0), |&index| {
            bookmark(
                index,
                (index < depth).then_some(index + 1),
                (index == 0).then_some(depth + 1),
            )
        });
        assert_eq!(items.len(), depth + 2);
        for (index, item) in items[..=depth].iter().enumerate() {
            assert_eq!(item.level, index.min(MAX_OUTLINE_LEVEL as usize) as u8);
        }
        assert_eq!(items[depth + 1].level, 0);
    }

    #[test]
    fn bookmark_titles_are_cut_at_a_unicode_character_boundary() {
        let title = format!("{}🦀é", "a".repeat(MAX_TITLE_CHARS - 1));
        let items = flatten(Some(0), |&index| {
            let mut node = bookmark(index, None, None);
            node.title = Some(title.clone());
            node
        });
        assert_eq!(
            items[0].title,
            format!("{}🦀", "a".repeat(MAX_TITLE_CHARS - 1))
        );
        assert_eq!(items[0].title.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn an_outline_stops_after_the_first_ten_thousand_readable_bookmarks() {
        let mut reads = 0;
        let items = flatten(Some(0), |&index| {
            reads += 1;
            let mut node = bookmark(index, None, Some(index + 1));
            if index == 0 {
                node.title = None;
            }
            node
        });
        assert_eq!(items.len(), MAX_OUTLINE_ITEMS);
        assert_eq!(reads, MAX_OUTLINE_ITEMS + 1);
        assert_eq!(items[0].title, "1");
        assert_eq!(
            items[MAX_OUTLINE_ITEMS - 1].title,
            MAX_OUTLINE_ITEMS.to_string()
        );
    }

    #[test]
    fn bookmark_cycles_are_visited_once_even_when_unreadable() {
        for unreadable in [false, true] {
            let mut reads = 0;
            let items = flatten(Some(0), |&index| {
                reads += 1;
                let mut node = bookmark(index, Some(index), Some(1 - index));
                node.title = (!unreadable).then(|| index.to_string());
                node
            });
            assert_eq!(reads, 2);
            assert_eq!(items.len(), if unreadable { 0 } else { 2 });
        }
    }
}
