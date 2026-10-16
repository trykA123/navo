use gpui_kit::{Point, Size, point, size};

const MIN_SCALE: f32 = 0.01;
const MAX_SCALE: f32 = 32.;
pub(super) const STEP: f32 = 1.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::window_view::viewer) struct Viewport {
    pub origin: Point<f32>,
    pub size: Size<f32>,
    pub device: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            origin: point(0., 0.),
            size: size(0., 0.),
            device: 1.,
        }
    }
}

impl Viewport {
    fn centre(&self) -> Point<f32> {
        point(self.size.width / 2., self.size.height / 2.)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.size.width <= 0. || self.size.height <= 0.
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::window_view::viewer) struct Zoom {
    pub image: Size<f32>,
    pub upscales: bool,
    pub fit: bool,
    pub scale: f32,
    pub pan: Point<f32>,
}

fn slack(shown: f32, room: f32) -> f32 {
    ((shown - room) / 2.).max(0.)
}

impl Zoom {
    pub(super) fn new(width: u32, height: u32, upscales: bool) -> Self {
        Self {
            image: size(width.max(1) as f32, height.max(1) as f32),
            upscales,
            fit: true,
            scale: 1.,
            pan: point(0., 0.),
        }
    }

    fn fit_scale(&self, viewport: &Viewport) -> f32 {
        if viewport.is_empty() {
            return 1.;
        }
        let wide = viewport.size.width * viewport.device / self.image.width;
        let tall = viewport.size.height * viewport.device / self.image.height;
        let fit = wide.min(tall);
        let fit = if self.upscales { fit } else { fit.min(1.) };
        fit.clamp(MIN_SCALE, MAX_SCALE)
    }

    pub(super) fn scale(&self, viewport: &Viewport) -> f32 {
        if self.fit {
            self.fit_scale(viewport)
        } else {
            self.scale
        }
    }

    pub(super) fn percent(&self, viewport: &Viewport) -> u32 {
        (self.scale(viewport) * 100.).round() as u32
    }

    pub(super) fn label(&self, viewport: &Viewport) -> String {
        let percent = self.percent(viewport);
        if self.fit {
            format!("Fit · {percent} %")
        } else {
            format!("{percent} %")
        }
    }

    pub(super) fn shown(&self, viewport: &Viewport) -> Size<f32> {
        let factor = self.scale(viewport) / viewport.device.max(f32::EPSILON);
        size(self.image.width * factor, self.image.height * factor)
    }

    pub(super) fn placement(&self, viewport: &Viewport) -> (Point<f32>, Size<f32>) {
        let shown = self.shown(viewport);
        let centre = viewport.centre();
        let origin = point(
            viewport.origin.x + centre.x - shown.width / 2. + self.pan.x,
            viewport.origin.y + centre.y - shown.height / 2. + self.pan.y,
        );
        (origin, shown)
    }

    pub(super) fn fit(&mut self) {
        self.fit = true;
        self.pan = point(0., 0.);
    }

    pub(super) fn zoom_to(&mut self, scale: f32, anchor: Point<f32>, viewport: &Viewport) {
        let old = self.scale(viewport);
        let new = scale.clamp(MIN_SCALE, MAX_SCALE);
        let centre = viewport.centre();
        let ratio = new / old;
        self.pan = point(
            (anchor.x - centre.x) - (anchor.x - centre.x - self.pan.x) * ratio,
            (anchor.y - centre.y) - (anchor.y - centre.y - self.pan.y) * ratio,
        );
        self.fit = false;
        self.scale = new;
        self.clamp(viewport);
    }

    pub(super) fn zoom_by(&mut self, factor: f32, anchor: Point<f32>, viewport: &Viewport) {
        let scale = self.scale(viewport) * factor;
        self.zoom_to(scale, anchor, viewport);
    }

    pub(super) fn centre_of(viewport: &Viewport) -> Point<f32> {
        viewport.centre()
    }

    pub(super) fn pan_by(&mut self, delta: Point<f32>, viewport: &Viewport) {
        self.pan = point(self.pan.x + delta.x, self.pan.y + delta.y);
        self.clamp(viewport);
    }

    pub(super) fn clamp(&mut self, viewport: &Viewport) {
        let shown = self.shown(viewport);
        let x = slack(shown.width, viewport.size.width);
        let y = slack(shown.height, viewport.size.height);
        self.pan = point(self.pan.x.clamp(-x, x), self.pan.y.clamp(-y, y));
    }
}
