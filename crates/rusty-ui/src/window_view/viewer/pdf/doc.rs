use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::{RenderImage, Task, size};
use rusty_pdf::protocol::{PageText, PdfError};

use super::jobs::{Done, Job, Shared};
use super::layout::{Camera, Pages};
use super::search::Hits;
use super::shots::{PAGE_BUDGET, Shot, Shots, THUMB_BUDGET};
use super::source::PdfPages;
use super::text::{Boxes, PageWords, TextIndex};
use crate::primitives::ListScroll;

const MARGIN: u32 = 1;
const MAX_WIDTH: u32 = 4096;
const MIN_WIDTH: u32 = 16;
pub(super) const THUMB_WIDTH: u32 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Selection {
    pub page: u32,
    pub from: u32,
    pub to: u32,
}

#[derive(Default)]
pub(super) struct Lists {
    pub hit_row: usize,
    pub page_row: usize,
    pub hit_scroll: ListScroll,
    pub page_scroll: ListScroll,
    pub thumbs_shown: Rc<Cell<(u32, u32)>>,
}

pub(super) struct Doc {
    shared: Shared,
    pub pages: Pages,
    pub camera: Camera,
    pub shots: Shots,
    pub thumbs: Shots,
    pub boxes: Boxes,
    pub index: TextIndex,
    pub hits: Hits,
    pub broken: BTreeMap<u32, PdfError>,
    pub unread: BTreeSet<u32>,
    pub selection: Option<Selection>,
    pub is_dragging: bool,
    pub lists: Lists,
    pub busy: Option<Task<()>>,
}

pub(super) struct Wants {
    pub index: bool,
    pub thumbs: bool,
}

impl Doc {
    pub(super) fn new(pages: Box<dyn PdfPages>) -> Self {
        let count = pages.page_count();
        Self {
            shared: Arc::new(Mutex::new(pages)),
            pages: Pages::new(count),
            camera: Camera::default(),
            shots: Shots::new(PAGE_BUDGET),
            thumbs: Shots::new(THUMB_BUDGET),
            boxes: Boxes::default(),
            index: TextIndex::new(count),
            hits: Hits::default(),
            broken: BTreeMap::new(),
            unread: BTreeSet::new(),
            selection: None,
            is_dragging: false,
            lists: Lists::default(),
            busy: None,
        }
    }

    pub(super) fn shared(&self) -> Shared {
        self.shared.clone()
    }

    pub(super) fn count(&self) -> u32 {
        self.pages.count()
    }

    pub(super) fn scale(&self) -> f32 {
        self.camera.scale(&self.pages)
    }

    pub(super) fn current_page(&self) -> u32 {
        self.camera.current_page(&self.pages)
    }

    pub(super) fn shown(&self) -> Range<u32> {
        let camera = &self.camera;
        self.pages
            .visible(camera.scroll.y, camera.viewport.height, self.scale())
    }

    fn near(&self) -> Vec<u32> {
        let shown = self.shown();
        let from = shown.start.saturating_sub(MARGIN);
        let to = (shown.end + MARGIN).min(self.count());
        let current = self.current_page();
        let mut near: Vec<u32> = (from..to).collect();
        near.sort_by_key(|page| page.abs_diff(current));
        near
    }

    pub(super) fn keep(&self) -> Range<u32> {
        let shown = self.shown();
        shown.start.saturating_sub(MARGIN)..shown.end + MARGIN
    }

    fn width_of(&self, page: u32) -> u32 {
        let wide = self.pages.size(page).width * self.scale() * self.camera.device;
        (wide.round() as u32).clamp(MIN_WIDTH, MAX_WIDTH)
    }

    fn page_job(&self, near: &[u32]) -> Option<Job> {
        if self.camera.viewport.width <= 0. {
            return None;
        }
        near.iter()
            .copied()
            .filter(|page| !self.broken.contains_key(page))
            .map(|page| (page, self.width_of(page)))
            .find(|(page, width)| self.shots.needs(*page, *width))
            .map(|(page, width)| Job::Page { page, width })
    }

    fn thumb_job(&self) -> Option<Job> {
        let (from, to) = self.lists.thumbs_shown.get();
        (from..to.min(self.count()))
            .filter(|page| !self.broken.contains_key(page))
            .find(|page| self.thumbs.get(*page).is_none())
            .map(|page| Job::Thumb {
                page,
                width: THUMB_WIDTH,
            })
    }

    pub(super) fn next_job(&self, wants: &Wants) -> Option<Job> {
        if self.count() == 0 {
            return None;
        }
        if !self.pages.has_size() && !self.unread.contains(&0) {
            return Some(Job::Text { page: 0 });
        }
        let near = self.near();
        let page = self.page_job(&near);
        let text = || {
            near.iter()
                .copied()
                .find(|page| self.boxes.get(*page).is_none() && !self.unread.contains(page))
                .map(|page| Job::Text { page })
        };
        let thumb = || self.thumb_job().filter(|_| wants.thumbs);
        let index = || {
            self.index
                .next_missing()
                .filter(|_| wants.index)
                .map(|page| Job::Text { page })
        };
        page.or_else(text).or_else(thumb).or_else(index)
    }

    pub(super) fn read(&mut self, page: u32, text: Arc<PageText>) -> bool {
        let found = size(text.width(), text.height());
        let anchor = self.pages.anchor(self.camera.scroll.y, self.scale());
        if self.pages.set_size(page, found) {
            self.camera.keep_anchor(&self.pages, anchor);
        }
        let is_new = self.index.insert(page, PageWords::of(&text));
        self.boxes.insert(page, text);
        is_new
    }

    pub(super) fn apply(&mut self, done: Done) -> (Option<u32>, Vec<Arc<RenderImage>>) {
        let keep = self.keep();
        match done {
            Done::Drawn(Job::Thumb { page, .. }, picture) => {
                let shot = Shot::new(picture.width, picture.height, picture.image);
                (None, self.thumbs.insert(page, shot, &(0..0)))
            }
            Done::Drawn(job, picture) => {
                let shot = Shot::new(picture.width, picture.height, picture.image);
                (None, self.shots.insert(job.page(), shot, &keep))
            }
            Done::Read(page, text) => (self.read(page, text).then_some(page), Vec::new()),
            Done::Failed(Job::Text { page }, _) => {
                self.unread.insert(page);
                self.index.skip(page);
                (None, Vec::new())
            }
            Done::Failed(job, error) => {
                self.broken.insert(job.page(), error);
                (None, Vec::new())
            }
        }
    }

    pub(super) fn drain_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut images = self.shots.drain();
        images.extend(self.thumbs.drain());
        images
    }
}
