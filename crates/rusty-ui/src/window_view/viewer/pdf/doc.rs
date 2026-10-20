use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::{Point, RenderImage, Task, size};
use rusty_pdf::protocol::{PageLinks, PageText};

use super::chapters::Chapters;
use super::hits::{HitKey, Hits};
use super::jobs::{Done, Job, Shared};
use super::layout::{Camera, Pages};
use super::shots::{PAGE_BUDGET, PREVIEW_BUDGET, Pool, Shot, Shots, THUMB_BUDGET};
use super::source::PdfPages;
use super::text::{Boxes, Kept, PageWords, TextIndex};
use super::trail::Trail;
use super::trouble::{Trouble, Verdict};
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
    pub hit_key: Option<HitKey>,
    pub page_row: usize,
    pub outline_row: usize,
    pub hit_scroll: ListScroll,
    pub page_scroll: ListScroll,
    pub outline_scroll: ListScroll,
    pub thumbs_shown: Rc<Cell<(u32, u32)>>,
}

pub(super) struct Doc {
    shared: Shared,
    pub pages: Pages,
    pub camera: Camera,
    pub shots: Shots,
    pub thumbs: Shots,
    pub boxes: Boxes,
    pub links: Kept<Option<PageLinks>>,
    pub trail: Trail,
    pub chapters: Chapters,
    pub press: Option<Point<f32>>,
    pub index: TextIndex,
    pub hits: Hits,
    pub trouble: Trouble,
    pub rail: RefCell<Option<RailSlots>>,
    pub selection: Option<Selection>,
    pub is_dragging: bool,
    pub lists: Lists,
    pub busy: Option<Task<()>>,
}

pub(super) type RailSlots = ((u64, u64), Vec<u32>);

pub(super) struct Wants {
    pub index: bool,
    pub thumbs: bool,
    pub chapters: bool,
    pub first_only: bool,
}

#[derive(Default)]
pub(super) struct Applied {
    pub read: Option<u32>,
    pub dropped: Vec<Arc<RenderImage>>,
    pub verdict: Option<Verdict>,
}

impl Doc {
    pub(super) fn new(pages: Box<dyn PdfPages>, is_preview: bool) -> Self {
        let count = pages.page_count();
        let budget = if is_preview {
            PREVIEW_BUDGET
        } else {
            PAGE_BUDGET
        };
        Self {
            shared: Arc::new(Mutex::new(pages)),
            pages: Pages::new(count),
            camera: Camera::default(),
            shots: Shots::new(budget, Pool::shared()),
            thumbs: Shots::new(THUMB_BUDGET, Pool::shared()),
            boxes: Boxes::default(),
            links: Kept::default(),
            trail: Trail::default(),
            chapters: Chapters::default(),
            press: None,
            index: TextIndex::new(count),
            hits: Hits::default(),
            trouble: Trouble::default(),
            rail: RefCell::default(),
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

    fn near(&self, first_only: bool) -> Vec<u32> {
        let shown = self.shown();
        if first_only {
            return shown.take(1).collect();
        }
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
            .filter(|page| !self.trouble.broken.contains_key(page))
            .map(|page| (page, self.width_of(page)))
            .find(|(page, width)| self.shots.needs(*page, *width))
            .map(|(page, width)| Job::Page { page, width })
    }

    fn thumb_job(&self) -> Option<Job> {
        let (from, to) = self.lists.thumbs_shown.get();
        (from..to.min(self.count()))
            .filter(|page| !self.trouble.broken.contains_key(page))
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
        if !self.pages.has_size() && !self.trouble.unread.contains(&0) {
            return Some(Job::Text { page: 0 });
        }
        let near = self.near(wants.first_only);
        let page = self.page_job(&near);
        if wants.first_only {
            return page;
        }
        let text = || {
            near.iter()
                .copied()
                .find(|page| self.boxes.get(*page).is_none() && !self.trouble.unread.contains(page))
                .map(|page| Job::Text { page })
        };
        let linked = || {
            near.iter()
                .copied()
                .find(|page| {
                    self.boxes.get(*page).is_some()
                        && self.links.get(*page).is_none()
                        && !self.trouble.unread.contains(page)
                })
                .map(|page| Job::Links { page })
        };
        let chapters = || (wants.chapters && !self.chapters.is_read()).then_some(Job::Outline);
        let thumb = || self.thumb_job().filter(|_| wants.thumbs);
        let index = || {
            self.index
                .next_missing()
                .filter(|_| wants.index)
                .map(|page| Job::Text { page })
        };
        page.or_else(text)
            .or_else(linked)
            .or_else(chapters)
            .or_else(thumb)
            .or_else(index)
    }

    pub(super) fn links_on(&self, page: u32) -> Option<&PageLinks> {
        self.links.get(page)?.as_ref().as_ref()
    }

    pub(super) fn read(&mut self, page: u32, text: Arc<PageText>) -> bool {
        let found = size(text.width(), text.height());
        let anchor = self.pages.anchor(self.camera.scroll.y, self.scale());
        if self.pages.set_size(page, found) {
            self.camera.keep_anchor(&self.pages, anchor);
        }
        let is_new = self.index.insert(page, PageWords::of(&text));
        let keep = self.keep();
        self.boxes.insert(page, text, &keep);
        is_new
    }

    fn drawn(&mut self, job: Job, width: u32, height: u32) {
        let page = job.page();
        let anchor = self.pages.anchor(self.camera.scroll.y, self.scale());
        if matches!(job, Job::Page { .. })
            && self
                .pages
                .set_aspect(page, height as f32 / width.max(1) as f32)
        {
            self.camera.keep_anchor(&self.pages, anchor);
        }
    }

    pub(super) fn apply(&mut self, done: Done) -> Applied {
        let keep = self.keep();
        let mut applied = Applied::default();
        match done {
            Done::Drawn(job, picture) => {
                self.trouble.succeeded();
                self.drawn(job, picture.width, picture.height);
                let shot = Shot::new(picture.width, picture.height, picture.image);
                applied.dropped = match job {
                    Job::Thumb { page, .. } => self.thumbs.insert(page, shot, &(0..0)),
                    _ => self.shots.insert(job.page(), shot, &keep),
                };
            }
            Done::Read(page, text) => {
                self.trouble.succeeded();
                applied.read = self.read(page, text).then_some(page);
            }
            Done::Linked(page, links) => {
                self.trouble.succeeded();
                self.links.insert(page, links, &keep);
            }
            Done::Outlined(items) => {
                self.trouble.succeeded();
                self.chapters = Chapters::Read(items);
                self.show_chapter();
            }
            Done::Failed(job, error) => {
                applied.verdict = self.trouble.failed(job, error);
                if self.trouble.unread.contains(&job.page()) {
                    self.index.skip(job.page());
                }
            }
        }
        applied
    }

    pub(super) fn show_chapter(&mut self) {
        let row = self.chapters.current(self.current_page()).unwrap_or(0);
        self.lists.outline_row = row;
        self.lists.outline_scroll.scroll_to(row);
    }

    pub(super) fn selected_hit_row(&self) -> usize {
        let key = self
            .lists
            .hit_key
            .or_else(|| self.hits.current().map(|hit| hit.key()));
        key.and_then(|key| self.hits.index_of(key)).unwrap_or(0)
    }

    pub(super) fn forget_drawing(&mut self) {
        self.trouble.forget_drawing();
    }

    pub(super) fn drain_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut images = self.shots.drain();
        images.extend(self.thumbs.drain());
        images
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::size;
    use rusty_pdf::protocol::{Bitmap, Outline, PdfError, RenderRequest};

    use super::*;

    struct Blank;

    impl PdfPages for Blank {
        fn page_count(&self) -> u32 {
            30
        }

        fn render(&mut self, _: RenderRequest) -> Result<Bitmap, PdfError> {
            Err(PdfError::TooLarge)
        }

        fn text(&mut self, _: u32) -> Result<PageText, PdfError> {
            Err(PdfError::Corrupt)
        }

        fn links(&mut self, _: u32) -> Result<PageLinks, PdfError> {
            Err(PdfError::Corrupt)
        }

        fn outline(&mut self) -> Result<Outline, PdfError> {
            Err(PdfError::Corrupt)
        }
    }

    #[test]
    fn a_resting_preview_draws_only_its_first_visible_page() {
        let mut doc = Doc::new(Box::new(Blank), true);
        doc.camera.viewport = size(800., 3000.);
        doc.pages.set_size(0, size(600., 800.));
        let wants = Wants {
            index: true,
            thumbs: true,
            chapters: true,
            first_only: true,
        };
        let job = doc.next_job(&wants).map(|job| job.page());
        assert_eq!(job, Some(0));
        assert!(matches!(doc.next_job(&wants), Some(Job::Page { .. })));
        doc.trouble.broken.insert(0, PdfError::TooLarge);
        assert!(doc.next_job(&wants).is_none());
    }
}
