pub(in crate::window_view::viewer) mod bar;
pub(in crate::window_view::viewer) mod count;
pub(in crate::window_view::viewer) mod crlf;
pub(in crate::window_view::viewer) mod find_all;
pub(in crate::window_view::viewer) mod history;
pub(in crate::window_view::viewer) mod jobs;
pub(in crate::window_view::viewer) mod marks;
pub(in crate::window_view::viewer) mod memory;
pub(in crate::window_view::viewer) mod overview;
pub(in crate::window_view::viewer) mod scan;

use std::ops::Range;

use rusty_core::marks::Bookmarks;
use rusty_core::paging::TextLayout;

use crate::prelude::*;
use crate::window_view::viewer::files::Reopen;
use count::CountRun;
use find_all::FindAll;
use history::History;
use marks::MarkLayer;
use memory::Memory;
use overview::Ticks;

pub(in crate::window_view::viewer) const FIND_CONTEXT: &str = "ViewerFind";
pub(in crate::window_view::viewer) const FIND_ALL_CONTEXT: &str = "FindAll";
pub(in crate::window_view::viewer) const GO_TO_CONTEXT: &str = "ViewerGoTo";

pub(in crate::window_view::viewer) struct Marking {
    pub source: Reopen,
    pub count: Option<CountRun>,
    pub marks: MarkLayer,
    pub bookmarks: Bookmarks,
    pub history: History,
    pub find_all: Option<FindAll>,
    pub line_ends: bool,
    pub rev: u64,
    pub layout: TextLayout,
    ticks: Ticks,
    memory: Memory,
}

impl Marking {
    pub(in crate::window_view::viewer) fn new(source: Reopen) -> Self {
        Self {
            source,
            count: None,
            marks: MarkLayer::default(),
            bookmarks: Bookmarks::default(),
            history: History::default(),
            find_all: None,
            line_ends: false,
            rev: 0,
            layout: TextLayout::default(),
            ticks: Ticks::default(),
            memory: Memory::default(),
        }
    }

    pub(in crate::window_view::viewer) fn set_layout(&mut self, layout: TextLayout) {
        self.layout = layout;
        self.marks.layout = layout;
    }

    pub(in crate::window_view::viewer) fn ticks(&self) -> &[(u32, u8)] {
        &self.ticks.shown
    }
}

pub(in crate::window_view::viewer) trait Marked:
    Sized + 'static
{
    fn marking(&mut self) -> &mut Marking;
    fn jump(&mut self, hit: Range<u64>, window: &mut Window, cx: &mut Context<Self>);
    fn refocus(&mut self, window: &mut Window, cx: &mut Context<Self>);
}

pub(in crate::window_view::viewer) fn toggle_bookmark<V: Marked>(
    view: &mut V,
    at: u64,
    cx: &mut Context<V>,
) {
    view.marking().bookmarks.toggle(at);
    marks::marks_changed(view, cx);
}

pub(in crate::window_view::viewer) fn source_changed<V: Marked>(
    view: &mut V,
    appended_to: Option<u64>,
    cx: &mut Context<V>,
) {
    count::count_source_changed(view, appended_to, cx);
    marks::marks_source_changed(view, appended_to, cx);
}

#[cfg(test)]
impl Marking {
    pub(in crate::window_view::viewer) fn running_scans(&self) -> usize {
        usize::from(
            self.count
                .as_ref()
                .is_some_and(count::CountRun::is_scanning),
        ) + usize::from(self.marks.is_scanning())
    }
}
