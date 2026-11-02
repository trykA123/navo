use std::ops::Range;

use gpui_kit::{Point, Size, point, size};

pub(super) const GAP: f32 = 12.;
pub(super) const EDGE: f32 = 24.;
pub(super) const ACTUAL: f32 = 96. / 72.;
pub(super) const STEP: f32 = 1.2;
const MIN_SCALE: f32 = 0.1;
const MAX_SCALE: f32 = 8. * ACTUAL;
const MAX_SIDE: f32 = 14_400.;
const A4: Size<f32> = Size {
    width: 595.,
    height: 842.,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Zoom {
    Fit,
    Scale(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Anchor {
    pub page: u32,
    pub fraction: f32,
}

pub(super) struct Pages {
    sizes: Vec<Size<f32>>,
    is_known: Vec<bool>,
    tops: Vec<f32>,
    widest: f32,
    any_known: bool,
    revision: u64,
}

impl Pages {
    pub(super) fn new(count: u32) -> Self {
        let count = count as usize;
        let mut pages = Self {
            sizes: vec![A4; count],
            is_known: vec![false; count],
            tops: Vec::new(),
            widest: A4.width,
            any_known: false,
            revision: 0,
        };
        pages.measure();
        pages
    }

    fn measure(&mut self) {
        self.tops.clear();
        self.tops.reserve(self.sizes.len() + 1);
        let mut top = 0.;
        for page in &self.sizes {
            self.tops.push(top);
            top += page.height;
        }
        self.tops.push(top);
        self.widest = self
            .sizes
            .iter()
            .fold(1., |wide, page| page.width.max(wide));
        self.revision += 1;
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn set_aspect(&mut self, page: u32, ratio: f32) -> bool {
        let at = page as usize;
        if at >= self.sizes.len() || self.is_known[at] || !ratio.is_finite() || ratio <= 0. {
            return false;
        }
        let width = self.sizes[at].width;
        let wanted = size(width, (width * ratio).min(MAX_SIDE));
        if (self.sizes[at].height - wanted.height).abs() < 0.5 {
            return false;
        }
        self.sizes[at] = wanted;
        self.measure();
        true
    }

    pub(super) fn count(&self) -> u32 {
        self.sizes.len() as u32
    }

    pub(super) fn has_size(&self) -> bool {
        self.any_known
    }

    pub(super) fn set_size(&mut self, page: u32, found: Size<f32>) -> bool {
        let at = page as usize;
        let found = size(found.width.min(MAX_SIDE), found.height.min(MAX_SIDE));
        let usable = found.width > 1. && found.height > 1. && found.width.is_finite();
        if at >= self.sizes.len() || !usable || (self.is_known[at] && self.sizes[at] == found) {
            return false;
        }
        if !self.any_known {
            self.any_known = true;
            self.sizes.fill(found);
        }
        self.sizes[at] = found;
        self.is_known[at] = true;
        self.measure();
        true
    }

    pub(super) fn size(&self, page: u32) -> Size<f32> {
        self.sizes.get(page as usize).copied().unwrap_or(A4)
    }

    pub(super) fn widest(&self) -> f32 {
        self.widest
    }

    pub(super) fn top(&self, page: u32, scale: f32) -> f32 {
        let at = (page as usize).min(self.sizes.len());
        self.tops[at] * scale + GAP * (at as f32 + 1.)
    }

    pub(super) fn height(&self, scale: f32) -> f32 {
        self.top(self.count(), scale)
    }

    pub(super) fn page_at(&self, y: f32, scale: f32) -> u32 {
        let count = self.count();
        if count == 0 {
            return 0;
        }
        let (mut low, mut high) = (0, count - 1);
        while low < high {
            let middle = (low + high).div_ceil(2);
            if self.top(middle, scale) - GAP <= y {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        low
    }

    pub(super) fn visible(&self, from: f32, span: f32, scale: f32) -> Range<u32> {
        if self.count() == 0 {
            return 0..0;
        }
        let first = self.page_at(from, scale);
        let last = self.page_at(from + span.max(0.), scale);
        first..last + 1
    }

    pub(super) fn anchor(&self, y: f32, scale: f32) -> Anchor {
        let page = self.page_at(y, scale);
        let height = (self.size(page).height * scale).max(1.);
        Anchor {
            page,
            fraction: (y - self.top(page, scale)) / height,
        }
    }

    pub(super) fn place(&self, anchor: Anchor, scale: f32) -> f32 {
        self.top(anchor.page, scale) + anchor.fraction * self.size(anchor.page).height * scale
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Camera {
    pub zoom: Zoom,
    pub scroll: Point<f32>,
    pub viewport: Size<f32>,
    pub device: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: Zoom::Fit,
            scroll: point(0., 0.),
            viewport: size(0., 0.),
            device: 1.,
        }
    }
}

impl Camera {
    pub(super) fn scale(&self, pages: &Pages) -> f32 {
        match self.zoom {
            Zoom::Fit if self.viewport.width > 2. * EDGE => {
                ((self.viewport.width - 2. * EDGE) / pages.widest()).clamp(MIN_SCALE, MAX_SCALE)
            }
            Zoom::Fit => ACTUAL,
            Zoom::Scale(scale) => scale,
        }
    }

    pub(super) fn percent(&self, pages: &Pages) -> u32 {
        (self.scale(pages) / ACTUAL * 100.).round() as u32
    }

    fn content_width(&self, pages: &Pages) -> f32 {
        pages.widest() * self.scale(pages) + 2. * EDGE
    }

    pub(super) fn clamp(&mut self, pages: &Pages) {
        let scale = self.scale(pages);
        let tall = (pages.height(scale) + GAP - self.viewport.height).max(0.);
        let wide = (self.content_width(pages) - self.viewport.width).max(0.);
        self.scroll = point(self.scroll.x.clamp(0., wide), self.scroll.y.clamp(0., tall));
    }

    pub(super) fn page_left(&self, pages: &Pages, page: u32) -> f32 {
        let scale = self.scale(pages);
        let width = pages.size(page).width * scale;
        let content = self.content_width(pages);
        if content <= self.viewport.width {
            (self.viewport.width - width) / 2.
        } else {
            (content - width) / 2. - self.scroll.x
        }
    }

    pub(super) fn current_page(&self, pages: &Pages) -> u32 {
        let probe = self.scroll.y + self.viewport.height / 3.;
        pages.page_at(probe, self.scale(pages))
    }

    pub(super) fn go_to(&mut self, pages: &Pages, page: u32) {
        let page = page.min(pages.count().saturating_sub(1));
        self.scroll.y = pages.top(page, self.scale(pages)) - GAP;
        self.clamp(pages);
    }

    pub(super) fn show_band(&mut self, pages: &Pages, page: u32, band: Range<f32>) {
        let scale = self.scale(pages);
        let top = pages.top(page, scale);
        let (from, to) = (top + band.start * scale, top + band.end * scale);
        if from < self.scroll.y || to > self.scroll.y + self.viewport.height {
            self.scroll.y = from - self.viewport.height / 3.;
            self.clamp(pages);
        }
    }

    pub(super) fn set_zoom(&mut self, pages: &Pages, zoom: Zoom, pointer: Point<f32>) {
        let before = self.scale(pages);
        let anchor = pages.anchor(self.scroll.y + pointer.y, before);
        let across = self.scroll.x + pointer.x - EDGE;
        self.zoom = match zoom {
            Zoom::Scale(scale) => Zoom::Scale(scale.clamp(MIN_SCALE, MAX_SCALE)),
            Zoom::Fit => Zoom::Fit,
        };
        let after = self.scale(pages);
        self.scroll = point(
            across * after / before + EDGE - pointer.x,
            pages.place(anchor, after) - pointer.y,
        );
        self.clamp(pages);
    }

    pub(super) fn zoom_by(&mut self, pages: &Pages, factor: f32, pointer: Point<f32>) {
        let scale = self.scale(pages) * factor;
        self.set_zoom(pages, Zoom::Scale(scale), pointer);
    }

    pub(super) fn keep_anchor(&mut self, pages: &Pages, anchor: Anchor) {
        self.scroll.y = pages.place(anchor, self.scale(pages));
        self.clamp(pages);
    }

    pub(super) fn middle(&self) -> Point<f32> {
        point(self.viewport.width / 2., self.viewport.height / 2.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(width: f32, height: f32) -> Camera {
        Camera {
            viewport: size(width, height),
            ..Camera::default()
        }
    }

    #[test]
    fn fit_fills_the_width_between_the_edges() {
        let pages = Pages::new(3);
        let camera = camera(595. + 2. * EDGE, 400.);
        assert!((camera.scale(&pages) - 1.).abs() < 1e-4);
    }

    #[test]
    fn the_first_known_size_becomes_every_unknown_size() {
        let mut pages = Pages::new(4);
        assert!(pages.set_size(2, size(100., 200.)));
        assert_eq!(pages.size(0), size(100., 200.));
        assert!(pages.set_size(3, size(200., 100.)));
        assert_eq!(
            (pages.size(1), pages.size(3)),
            (size(100., 200.), size(200., 100.))
        );
        assert_eq!(pages.widest(), 200.);
    }

    #[test]
    fn a_drawn_page_sets_its_height_until_its_text_gives_the_size() {
        let mut pages = Pages::new(2);
        assert!(pages.set_aspect(1, 2.));
        assert_eq!(pages.size(1), size(595., 1190.));
        pages.set_size(1, size(100., 150.));
        assert!(!pages.set_aspect(1, 2.));
        assert_eq!(pages.size(1), size(100., 150.));
    }

    #[test]
    fn page_sizes_stop_at_the_pdf_maximum() {
        let mut pages = Pages::new(1);
        pages.set_size(0, size(1e9, 300.));
        assert_eq!(pages.size(0), size(MAX_SIDE, 300.));
    }

    #[test]
    fn page_at_finds_the_page_under_a_document_offset() {
        let mut pages = Pages::new(3);
        pages.set_size(0, size(100., 100.));
        assert_eq!(pages.page_at(0., 1.), 0);
        assert_eq!(pages.page_at(GAP + 99., 1.), 0);
        assert_eq!(pages.page_at(2. * GAP + 101., 1.), 1);
        assert_eq!(pages.page_at(10_000., 1.), 2);
        assert_eq!(pages.visible(GAP + 50., 100., 1.), 0..2);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut pages = Pages::new(20);
        pages.set_size(0, size(500., 700.));
        let mut camera = camera(800., 600.);
        camera.go_to(&pages, 5);
        let pointer = point(300., 200.);
        let before = pages.anchor(camera.scroll.y + pointer.y, camera.scale(&pages));
        camera.zoom_by(&pages, 2., pointer);
        let after = pages.anchor(camera.scroll.y + pointer.y, camera.scale(&pages));
        assert_eq!(before.page, after.page);
        assert!((before.fraction - after.fraction).abs() < 1e-3);
    }

    #[test]
    fn go_to_puts_the_page_at_the_top_and_current_page_follows() {
        let mut pages = Pages::new(10);
        pages.set_size(0, size(500., 700.));
        let mut camera = camera(800., 600.);
        camera.go_to(&pages, 4);
        assert_eq!(camera.current_page(&pages), 4);
        camera.go_to(&pages, 99);
        assert_eq!(camera.current_page(&pages), 9);
    }
}
