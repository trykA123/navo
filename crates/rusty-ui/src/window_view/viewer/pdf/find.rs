use gpui_kit::Focusable;

use super::PdfView;
use super::doc::Doc;
use super::search::{Hit, Hits, Query, find};
use super::text::{PageWords, rects};
use crate::prelude::*;

const NO_MORE_YET: &str = "No more hits yet. Ferrite is still reading the pages.";
const TYPE_FIRST: &str = "Type the text to find first.";
const BAND: f32 = 12.;

fn hits_in(page: u32, words: &PageWords, query: &Query) -> Vec<Hit> {
    find(&words.chars, query)
        .into_iter()
        .map(|(start, len)| Hit {
            page,
            start,
            len,
            y: words.y_of(start),
        })
        .collect()
}

fn first_from(hits: &Hits, page: u32) -> Option<Hit> {
    hits.all()
        .find(|hit| hit.page >= page)
        .copied()
        .or_else(|| hits.wrap(false))
}

fn reveal(doc: &mut Doc, hit: Hit) {
    let band = doc
        .boxes
        .get(hit.page)
        .and_then(|text| {
            let boxes = rects(text, hit.start, hit.len);
            let top = boxes.iter().map(|bounds| bounds.top).reduce(f32::min)?;
            let bottom = boxes.iter().map(|bounds| bounds.bottom).reduce(f32::max)?;
            Some(top..bottom)
        })
        .unwrap_or_else(|| {
            let y = hit.y * doc.pages.size(hit.page).height;
            y - BAND..y + BAND
        });
    doc.camera.show_band(&doc.pages, hit.page, band);
    if let Some(row) = doc
        .hits
        .all()
        .position(|found| found.page == hit.page && found.start == hit.start)
    {
        doc.lists.hit_row = row;
        doc.lists.hit_scroll.scroll_to(row);
    }
}

fn search_all(doc: &mut Doc, query: &Query) {
    doc.hits.clear();
    if query.is_empty() {
        return;
    }
    let found: Vec<(u32, Vec<Hit>)> = doc
        .index
        .pages()
        .map(|(page, words)| (page, hits_in(page, words, query)))
        .collect();
    for (page, hits) in found {
        doc.hits.add_page(page, hits);
    }
    let first = first_from(&doc.hits, doc.current_page());
    doc.hits.set_current(first);
    if let Some(hit) = first {
        reveal(doc, hit);
    }
}

impl PdfView {
    pub(super) fn start_finding(&mut self, cx: &mut Context<Self>) {
        self.is_finding = true;
        cx.notify();
    }

    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.inputs.find.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.is_finding = true;
        cx.notify();
    }

    pub fn find_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs
            .find
            .update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
        self.typed_query(window, cx);
    }

    pub(super) fn typed_query(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.inputs.find.read(cx).value().to_string();
        if text != self.query.text {
            self.query.text = text;
            self.is_finding = true;
            self.rerun(cx);
        }
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        let query = self.query.clone();
        if let Some(doc) = self.doc_mut() {
            search_all(doc, &query);
        }
        cx.notify();
        self.pump(cx);
    }

    pub(super) fn search_page(&mut self, page: u32) {
        let query = self.query.clone();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let Some(words) = doc.index.get(page).filter(|_| !query.is_empty()) else {
            return;
        };
        let hits = hits_in(page, words, &query);
        doc.hits.add_page(page, hits);
        if doc.hits.current().is_none() {
            let first = first_from(&doc.hits, doc.current_page());
            doc.hits.set_current(first);
            if let Some(hit) = first {
                reveal(doc, hit);
            }
        }
    }

    pub(super) fn step(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.query.is_empty() {
            self.open_find(window, cx);
            return self.set_note(TYPE_FIRST, cx);
        }
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let from = doc
            .hits
            .current()
            .map_or((doc.current_page(), 0), |hit| (hit.page, hit.start));
        let wrapped = || doc.hits.wrap(backward).filter(|_| doc.index.is_complete());
        let next = doc.hits.step(from, backward).or_else(wrapped);
        match next {
            Some(hit) => {
                doc.hits.set_current(Some(hit));
                reveal(doc, hit);
                self.note = None;
                cx.notify();
            }
            None if doc.hits.total() == 0 && doc.index.is_complete() => {}
            None => self.set_note(NO_MORE_YET, cx),
        }
    }

    pub(super) fn go_to_hit(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let picked = doc.hits.all().nth(row).copied();
        if let Some(hit) = picked {
            doc.hits.set_current(Some(hit));
            reveal(doc, hit);
        }
        cx.notify();
    }

    pub(super) fn toggle_case(&mut self, cx: &mut Context<Self>) {
        self.query.match_case = !self.query.match_case;
        self.rerun(cx);
    }

    pub(super) fn toggle_word(&mut self, cx: &mut Context<Self>) {
        self.query.whole_word = !self.query.whole_word;
        self.rerun(cx);
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.is_finding && self.query.is_empty() {
            return false;
        }
        self.inputs
            .find
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.query.text.clear();
        self.is_finding = false;
        self.note = None;
        self.rerun(cx);
        self.focus_pages(window, cx);
        true
    }

    pub(super) fn count_label(&self) -> Option<SharedString> {
        let doc = self.doc()?;
        if self.query.is_empty() {
            return None;
        }
        Some(super::search::count_label(&doc.hits, doc.index.is_complete()).into())
    }
}
