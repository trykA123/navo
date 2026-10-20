mod cache;
mod paint;
mod tip;

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    Bounds, EventEmitter, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent,
    Role, canvas,
};
use rusty_automotive::space::{Owners, Region, ScaleMode};
use rusty_l10n::t;

use self::cache::{Hit, Key, Layout};
pub use self::cache::{Item, Step};
use self::paint::{Look, Scene};
use crate::prelude::*;
use crate::primitives::Tooltip;
use crate::tokens;

#[derive(Clone)]
pub struct Label {
    pub name: SharedString,
    pub object: Option<SharedString>,
}

#[derive(Clone)]
pub struct StripData {
    pub region: Arc<Region>,
    pub owners: Option<Arc<Owners>>,
    pub describe: Rc<dyn Fn(Item) -> Label>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripEvent {
    Hover(Option<usize>),
    Pick(usize),
}

pub struct MemoryStrip {
    data: StripData,
    height: Pixels,
    mode: ScaleMode,
    zoom: Option<usize>,
    caret: Option<usize>,
    lit: Option<Rc<[bool]>>,
    layout: Option<Rc<Layout>>,
    bounds: Bounds<Pixels>,
    hover: Option<Hit>,
    focus: FocusHandle,
}

impl EventEmitter<StripEvent> for MemoryStrip {}

impl Focusable for MemoryStrip {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl MemoryStrip {
    pub fn new(data: StripData, height: Pixels, cx: &mut Context<Self>) -> Self {
        Self {
            data,
            height,
            mode: ScaleMode::Compressed,
            zoom: None,
            caret: None,
            lit: None,
            layout: None,
            bounds: Bounds::default(),
            hover: None,
            focus: cx.focus_handle(),
        }
    }

    pub fn set_data(&mut self, data: StripData, cx: &mut Context<Self>) {
        self.data = data;
        self.zoom = None;
        self.caret = None;
        self.lit = None;
        self.hover = None;
        self.layout = None;
        self.relayout();
        cx.notify();
    }

    pub fn mode(&self) -> ScaleMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ScaleMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.relayout();
        cx.notify();
    }

    pub fn caret(&self) -> Option<usize> {
        self.caret
    }

    pub fn set_caret(&mut self, caret: Option<usize>, cx: &mut Context<Self>) {
        self.caret = caret.filter(|at| *at < self.data.region.blocks.len());
        cx.notify();
    }

    pub fn set_lit(&mut self, lit: Option<Rc<[bool]>>, cx: &mut Context<Self>) {
        self.lit = lit;
        cx.notify();
    }

    pub fn zoom(&self) -> Option<usize> {
        self.zoom
    }

    pub fn step(&mut self, step: Step, cx: &mut Context<Self>) -> bool {
        let Some(next) = self.layout.as_ref().and_then(|l| l.step(self.caret, step)) else {
            return false;
        };
        self.pick(next, cx);
        true
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) -> bool {
        let wide = |at: usize| self.data.region.blocks.get(at).is_some_and(|b| b.size > 0);
        let Some(at) = self.caret.filter(|at| wide(*at) && self.zoom != Some(*at)) else {
            return false;
        };
        self.zoom = Some(at);
        self.hover = None;
        self.relayout();
        cx.notify();
        true
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) -> bool {
        if self.zoom.take().is_none() {
            return false;
        }
        self.hover = None;
        self.relayout();
        cx.notify();
        true
    }

    fn pick(&mut self, block: usize, cx: &mut Context<Self>) {
        self.caret = Some(block);
        cx.emit(StripEvent::Pick(block));
        cx.notify();
    }

    fn key(&self) -> Option<Key> {
        Key::new(f32::from(self.bounds.size.width), self.mode, self.zoom)
    }

    fn relayout(&mut self) -> bool {
        let key = self.key();
        if self.layout.as_ref().map(|layout| layout.key) == key {
            return false;
        }
        let owners = self.data.owners.as_deref();
        self.layout = key.map(|key| Rc::new(Layout::build(&self.data.region, owners, key)));
        true
    }

    fn column(&self, x: Pixels) -> Option<u32> {
        let layout = self.layout.as_ref()?;
        let bounds = self.bounds;
        let width = f32::from(bounds.size.width);
        let along = f32::from(x - bounds.origin.x) / width;
        if !(0.0..1.0).contains(&along) {
            return None;
        }
        Some((along * layout.key.width as f32) as u32)
    }

    fn hit_at(&self, x: Pixels) -> Option<Hit> {
        let column = self.column(x)?;
        self.layout.as_ref()?.hit(&self.data.region, column)
    }

    fn hover_at(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let hit = self.hit_at(x);
        if hit == self.hover {
            return;
        }
        let block = hit.as_ref().map(|hit| hit.block);
        let changed = self.hover.as_ref().map(|hit| hit.block) != block;
        self.hover = hit;
        if changed {
            cx.emit(StripEvent::Hover(block));
        }
        cx.notify();
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        if self.hover.take().is_some() {
            cx.emit(StripEvent::Hover(None));
            cx.notify();
        }
    }

    fn press(&mut self, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(hit) = self.hit_at(x) {
            self.pick(hit.block, cx);
        }
    }

    fn measure(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        self.bounds = bounds;
        if self.relayout() {
            self.hover = None;
            cx.notify();
        }
    }

    fn scene(&self, look: Look) -> Option<Scene> {
        let layout = self.layout.clone()?;
        Some(Scene {
            caret: self.caret.and_then(|at| layout.span_of(at)),
            layout,
            lit: self.lit.clone(),
            look,
        })
    }

    fn tip(&self, hit: &Hit, factor: f32) -> AnyElement {
        let text = tip::text(&self.data, &hit.over);
        let shown = text.clone();
        div()
            .id(("strip-tip", hit.x.start as usize))
            .absolute()
            .top_0()
            .left(px(hit.x.start as f32 * factor))
            .w(px(((hit.x.end - hit.x.start) as f32 * factor).max(1.)))
            .h(self.height)
            .tooltip(move |window, cx| Tooltip::new(shown.clone()).build(window, cx))
            .into_any_element()
    }
}

fn surface(look: Look, cx: &mut Context<MemoryStrip>) -> impl IntoElement {
    let entity = cx.entity().downgrade();
    canvas(
        move |area, _, cx| {
            entity
                .update(cx, |strip, cx| {
                    strip.measure(area, cx);
                    strip.scene(look)
                })
                .ok()
                .flatten()
        },
        |area, scene, window, _| {
            if let Some(scene) = scene {
                paint::paint(&scene, area, window);
            }
        },
    )
    .absolute()
    .size_full()
}

impl Render for MemoryStrip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        let is_focused = self.focus.is_focused(window);
        let factor = self.layout.as_ref().map_or(1., |layout| {
            f32::from(self.bounds.size.width) / layout.key.width as f32
        });
        let tip = self.hover.as_ref().map(|hit| self.tip(hit, factor));
        let label = t!("space-strip-label", region = self.data.region.name.clone());
        div()
            .id("memory-strip")
            .role(Role::Group)
            .aria_label(label)
            .test_support()
            .track_focus(&self.focus)
            .relative()
            .w_full()
            .h(self.height + t.spacing.xxs * 3.)
            .when(is_focused, |strip| {
                strip.rounded(t.radii.xs).shadow(t.shadows.focus.clone())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|strip, event: &MouseDownEvent, window, cx| {
                    strip.press(event.position.x, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(|strip, event: &MouseMoveEvent, _, cx| {
                strip.hover_at(event.position.x, cx);
            }))
            .on_hover(cx.listener(|strip, hovered: &bool, _, cx| {
                if !*hovered {
                    strip.leave(cx);
                }
            }))
            .child(surface(Look::new(t, f32::from(self.height)), cx))
            .children(tip)
    }
}
