use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;

use rusty_pdf::protocol::{CharBox, PageText};

const BOXES_KEPT: usize = 24;
pub(super) const INDEX_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct PageWords {
    pub chars: Arc<[char]>,
    lines: Arc<[(u32, f32)]>,
}

impl PageWords {
    pub(super) fn of(text: &PageText) -> Self {
        let height = text.height().max(1.);
        let mut lines: Vec<(u32, f32)> = Vec::new();
        let mut last: Option<f32> = None;
        for (at, ch) in text.chars().iter().enumerate() {
            let top = ch.bounds.top;
            let tall = (ch.bounds.bottom - top).abs().max(1.);
            if last.is_none_or(|last| (top - last).abs() > tall / 2.) {
                lines.push((at as u32, (top / height).clamp(0., 1.)));
                last = Some(top);
            }
        }
        Self {
            chars: text.chars().iter().map(|ch| ch.ch).collect(),
            lines: lines.into(),
        }
    }

    pub(super) fn y_of(&self, at: u32) -> f32 {
        let line = self.lines.partition_point(|(start, _)| *start <= at);
        line.checked_sub(1)
            .and_then(|line| self.lines.get(line))
            .map_or(0.5, |(_, y)| *y)
    }
}

pub(super) struct TextIndex {
    pages: Vec<Option<PageWords>>,
    indexed: u32,
    next: u32,
    chars: usize,
    is_full: bool,
}

impl TextIndex {
    pub(super) fn new(count: u32) -> Self {
        Self {
            pages: (0..count).map(|_| None).collect(),
            indexed: 0,
            next: 0,
            chars: 0,
            is_full: false,
        }
    }

    pub(super) fn insert(&mut self, page: u32, words: PageWords) -> bool {
        let Some(slot) = self.pages.get_mut(page as usize) else {
            return false;
        };
        if slot.is_some() || self.is_full {
            return false;
        }
        self.chars += words.chars.len();
        if self.chars > INDEX_BUDGET {
            self.is_full = true;
            return false;
        }
        *slot = Some(words);
        self.indexed += 1;
        self.advance();
        true
    }

    fn advance(&mut self) {
        while self.get(self.next).is_some() {
            self.next += 1;
        }
    }

    pub(super) fn get(&self, page: u32) -> Option<&PageWords> {
        self.pages.get(page as usize).and_then(Option::as_ref)
    }

    pub(super) fn indexed(&self) -> u32 {
        self.indexed
    }

    pub(super) fn is_full(&self) -> bool {
        self.is_full
    }

    pub(super) fn is_complete(&self) -> bool {
        self.is_full || self.next as usize >= self.pages.len()
    }

    pub(super) fn next_missing(&self) -> Option<u32> {
        (!self.is_complete()).then_some(self.next)
    }

    pub(super) fn skip(&mut self, page: u32) {
        if page == self.next {
            self.next += 1;
            self.advance();
        }
    }

    pub(super) fn pages(&self) -> impl Iterator<Item = (u32, &PageWords)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(page, words)| Some((page as u32, words.as_ref()?)))
    }
}

pub(super) struct Kept<T> {
    kept: VecDeque<(u32, Arc<T>)>,
}

pub(super) type Boxes = Kept<PageText>;

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self {
            kept: VecDeque::new(),
        }
    }
}

impl<T> Kept<T> {
    pub(super) fn get(&self, page: u32) -> Option<&Arc<T>> {
        self.kept
            .iter()
            .find(|(at, _)| *at == page)
            .map(|(_, kept)| kept)
    }

    pub(super) fn insert(&mut self, page: u32, value: Arc<T>, keep: &Range<u32>) {
        self.kept.retain(|(at, _)| *at != page);
        self.kept.push_back((page, value));
        while self.kept.len() > BOXES_KEPT {
            let Some(at) = self.kept.iter().position(|(at, _)| !keep.contains(at)) else {
                break;
            };
            self.kept.remove(at);
        }
    }
}

fn distance(bounds: &CharBox, x: f32, y: f32) -> f32 {
    let dx = (bounds.left - x).max(x - bounds.right).max(0.);
    let dy = (bounds.top - y).max(y - bounds.bottom).max(0.);
    dy * 4. + dx
}

pub(super) fn char_at(text: &PageText, x: f32, y: f32) -> Option<u32> {
    text.chars()
        .iter()
        .enumerate()
        .filter(|(_, ch)| !ch.ch.is_control())
        .min_by(|(_, a), (_, b)| distance(&a.bounds, x, y).total_cmp(&distance(&b.bounds, x, y)))
        .map(|(at, _)| at as u32)
}

pub(super) fn rects(text: &PageText, start: u32, len: u32) -> Vec<CharBox> {
    let end = (start + len) as usize;
    let mut merged: Vec<CharBox> = Vec::new();
    for ch in text
        .chars()
        .get(start as usize..end.min(text.chars().len()))
        .unwrap_or(&[])
    {
        if ch.ch.is_control() {
            continue;
        }
        let bounds = ch.bounds;
        match merged.last_mut() {
            Some(line)
                if (line.top - bounds.top).abs() < (bounds.bottom - bounds.top).abs() / 2. =>
            {
                line.left = line.left.min(bounds.left);
                line.right = line.right.max(bounds.right);
                line.top = line.top.min(bounds.top);
                line.bottom = line.bottom.max(bounds.bottom);
            }
            _ => merged.push(bounds),
        }
    }
    merged
}

pub(super) fn copied(text: &PageText, from: u32, to: u32) -> String {
    let (start, end) = (from.min(to) as usize, from.max(to) as usize + 1);
    let picked = text
        .chars()
        .get(start..end.min(text.chars().len()))
        .unwrap_or(&[]);
    picked
        .iter()
        .map(|ch| ch.ch)
        .filter(|ch| *ch != '\r')
        .collect()
}

#[cfg(test)]
mod tests {
    use rusty_pdf::protocol::TextChar;

    use super::*;

    fn line(text: &str, top: f32) -> Vec<TextChar> {
        text.chars()
            .enumerate()
            .map(|(at, ch)| TextChar {
                ch,
                bounds: CharBox {
                    left: at as f32 * 10.,
                    top,
                    right: at as f32 * 10. + 9.,
                    bottom: top + 12.,
                },
            })
            .collect()
    }

    fn page() -> PageText {
        let mut chars = line("alpha", 10.);
        chars.extend(line("beta", 50.));
        PageText::new(100., 100., chars).expect("page")
    }

    #[test]
    fn lines_give_each_hit_its_height_on_the_page() {
        let words = PageWords::of(&page());
        assert_eq!((words.y_of(2), words.y_of(6)), (0.1, 0.5));
    }

    #[test]
    fn the_pointer_picks_the_nearest_character_and_copies_a_range() {
        let page = page();
        assert_eq!(char_at(&page, 21., 15.), Some(2));
        assert_eq!(char_at(&page, 500., 52.), Some(8));
        assert_eq!(copied(&page, 8, 5), "beta");
    }

    #[test]
    fn a_hit_over_two_lines_draws_two_rectangles() {
        let boxes = rects(&page(), 3, 3);
        assert_eq!(boxes.len(), 2);
        assert_eq!((boxes[0].left, boxes[0].right), (30., 49.));
    }

    #[test]
    fn visible_pages_keep_their_boxes_when_older_pages_leave() {
        let mut boxes = Boxes::default();
        let text = Arc::new(page());
        for at in 0..BOXES_KEPT as u32 + 4 {
            boxes.insert(at, text.clone(), &(0..2));
        }
        assert!(boxes.get(0).is_some() && boxes.get(1).is_some());
        assert!(boxes.get(2).is_none());
        assert_eq!(boxes.kept.len(), BOXES_KEPT);
    }

    #[test]
    fn the_index_stops_at_its_budget_and_counts_what_it_holds() {
        let mut index = TextIndex::new(3);
        assert!(index.insert(1, PageWords::of(&page())));
        assert_eq!(index.next_missing(), Some(0));
        index.skip(0);
        assert_eq!(index.next_missing(), Some(2));
        assert_eq!((index.indexed(), index.is_complete()), (1, false));
        index.skip(2);
        assert!(index.is_complete());
    }
}
