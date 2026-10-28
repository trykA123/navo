use gpui_kit::base::TestSupportExt;
use gpui_kit::{DispatchPhase, Role, canvas};
use rusty_core::{Axis, Command, Layout, PaneId, Rect, SplitInfo};

use super::FerriteWindow;
use crate::prelude::*;
use crate::tokens;

const MIN_PANE_WIDTH: Pixels = px(280.);
const MIN_PANE_HEIGHT: Pixels = px(160.);
const FULL: Rect = Rect {
    x: 0.,
    y: 0.,
    width: 1.,
    height: 1.,
};

struct DividerDrag {
    split: SplitInfo,
    area: Bounds<Pixels>,
    ratio: u16,
    has_moved: bool,
}

#[derive(Default)]
pub(super) struct TilesState {
    area: Bounds<Pixels>,
    gap: Pixels,
    drag: Option<DividerDrag>,
    pub(super) switcher: Option<super::layout_switcher::SwitcherState>,
    pub(super) fades: super::layout_mode::Fades,
    pub(super) presets: super::preset_edit::PresetDialog,
}

impl TilesState {
    pub(super) fn fits(&self, layout: &Layout) -> bool {
        self.area.size.width <= px(0.)
            || layout.rects().iter().all(|(_, rect)| {
                self.area.size.width * rect.width as f32 - self.gap >= MIN_PANE_WIDTH
                    && self.area.size.height * rect.height as f32 - self.gap >= MIN_PANE_HEIGHT
            })
    }
}

impl DividerDrag {
    fn move_to(&mut self, position: Point<Pixels>) {
        let rect = self.split.rect;
        let (offset, length) = match self.split.axis {
            Axis::SideBySide => (
                position.x - self.area.left() - self.area.size.width * rect.x as f32,
                self.area.size.width * rect.width as f32,
            ),
            Axis::Stacked => (
                position.y - self.area.top() - self.area.size.height * rect.y as f32,
                self.area.size.height * rect.height as f32,
            ),
        };
        if length > px(0.) {
            self.ratio = ((offset / length * 1000.).round() as u16).clamp(100, 900);
        }
    }
}

pub(super) fn positioned(rect: Rect) -> Div {
    div()
        .absolute()
        .left(relative(rect.x as f32))
        .top(relative(rect.y as f32))
        .w(relative(rect.width as f32))
        .h(relative(rect.height as f32))
}

impl FerriteWindow {
    pub(super) fn core_layout(&self) -> Option<&Layout> {
        let state = self.browser.state();
        state
            .window(state.focused_window())
            .map(|window| window.layout())
    }

    pub(super) fn root_ratio(&self) -> f32 {
        self.core_layout()
            .and_then(|layout| {
                layout
                    .splits()
                    .first()
                    .map(|split| f32::from(split.ratio) / 1000.)
            })
            .unwrap_or(0.5)
    }

    pub(super) fn tiles_layout(&self) -> Layout {
        self.core_layout()
            .cloned()
            .unwrap_or_else(|| Layout::new(self.browser.state().focused_pane()))
    }

    fn dragged_layout(&self) -> Layout {
        let mut layout = self.tiles_layout();
        if let Some(drag) = &self.tiles.drag {
            let _ = layout.set_ratio(&drag.split.path, drag.ratio);
        }
        layout
    }

    fn dragged_split(&self, split: &SplitInfo) -> Option<SplitInfo> {
        let drag = self.tiles.drag.as_ref()?;
        (drag.split.path == split.path).then(|| SplitInfo {
            ratio: drag.ratio,
            ..split.clone()
        })
    }

    pub(super) fn is_one_pane(&self, window: &Window) -> bool {
        self.shown_panes(window).len() == 1 && self.tiles_layout().panes().len() > 1
    }

    pub(super) fn tiles_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let gap = tokens::current(cx).spacing.xs;
        let layout = self.tiles_layout();
        let shown = self.shown_panes(window);
        let is_one_pane = shown.len() == 1 && layout.panes().len() > 1;
        let covers = match is_one_pane {
            true => Vec::new(),
            false => self.tiles.fades.covers(tokens::current(cx), &layout),
        };
        let panes: Vec<_> = layout
            .rects()
            .into_iter()
            .filter(|(pane, _)| shown.contains(pane))
            .map(|(pane, rect)| match is_one_pane {
                true => self.one_pane_tile(pane, window, cx),
                false => positioned(rect)
                    .p(gap / 2.)
                    .overflow_hidden()
                    .child(self.pane_view(pane, window, cx)),
            })
            .collect();
        let dividers: Vec<_> = layout
            .splits()
            .iter()
            .filter(|_| shown.len() > 1)
            .map(|split| self.divider(split, cx))
            .collect();
        div()
            .id("tiles")
            .test_support()
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .children(panes)
            .children(covers)
            .children(dividers)
            .child(self.tiles_events(gap, cx))
            .into_any_element()
    }

    fn one_pane_tile(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let gap = tokens::current(cx).spacing.xs;
        let strip = self.pane_strip(&self.tiles_layout(), cx);
        positioned(FULL)
            .p(gap / 2.)
            .flex()
            .flex_col()
            .child(strip)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.pane_view(pane, window, cx)),
            )
    }

    fn divider(&self, split: &SplitInfo, cx: &mut Context<Self>) -> AnyElement {
        let t = tokens::current(cx);
        let gap = t.spacing.xs;
        let dragged = self.dragged_split(split);
        let line = divider_line(t, split.axis, dragged.is_some());
        let split = dragged.as_ref().unwrap_or(split);
        positioned(split.rect)
            .id(SharedString::from(format!("divider-{}", split.path)))
            .role(Role::Splitter)
            .aria_label(rusty_l10n::t!(
                "columns-resize",
                column = rusty_l10n::tr("command-center-context-pane")
            ))
            .group("tile-divider")
            .flex()
            .items_center()
            .justify_center()
            .map(|handle| divider_position(handle, split, gap))
            .map(|handle| Self::wire_divider(handle, split, cx))
            .test_support()
            .child(line)
            .into_any_element()
    }

    fn wire_divider(
        handle: gpui_kit::Stateful<Div>,
        split: &SplitInfo,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let start = split.clone();
        let path = split.path.clone();
        handle
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    this.tiles.drag = Some(DividerDrag {
                        split: start.clone(),
                        area: this.tiles.area,
                        ratio: start.ratio,
                        has_moved: false,
                    });
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.click_count() >= 2 {
                    this.apply(
                        &Command::LayoutRatio {
                            split: path.clone(),
                            ratio: 500,
                        },
                        window,
                        cx,
                    );
                }
            }))
    }

    fn tiles_events(&self, gap: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let measure = cx.listener(move |this, bounds: &Bounds<Pixels>, window, cx| {
            if this.tiles.area != *bounds || this.tiles.gap != gap {
                this.tiles.area = *bounds;
                this.tiles.gap = gap;
                let entity = cx.entity().entity_id();
                window.on_next_frame(move |_, cx| cx.notify(entity));
            }
        });
        let preview = cx.listener(Self::preview_divider);
        let release = cx.listener(Self::release_divider);
        let preview = move |event: &MouseMoveEvent, phase, window: &mut Window, cx: &mut App| {
            if phase == DispatchPhase::Capture {
                preview(event, window, cx);
            }
        };
        let release = move |event: &MouseUpEvent, phase, window: &mut Window, cx: &mut App| {
            if phase == DispatchPhase::Capture {
                release(event, window, cx);
            }
        };
        canvas(
            move |bounds, window, cx| measure(&bounds, window, cx),
            move |_, (), window, _| {
                window.on_mouse_event(preview);
                window.on_mouse_event(release);
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn preview_divider(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(drag) = &mut self.tiles.drag else {
            return;
        };
        let previous = drag.ratio;
        drag.has_moved = true;
        drag.move_to(event.position);
        if !self.tiles.fits(&self.dragged_layout())
            && let Some(drag) = &mut self.tiles.drag
        {
            drag.ratio = previous;
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn release_divider(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        if let Some(drag) = self.tiles.drag.take().filter(|drag| drag.has_moved) {
            self.apply(
                &Command::LayoutRatio {
                    split: drag.split.path,
                    ratio: drag.ratio,
                },
                window,
                cx,
            );
        }
    }
}

fn divider_position(
    handle: gpui_kit::Stateful<Div>,
    split: &SplitInfo,
    gap: Pixels,
) -> gpui_kit::Stateful<Div> {
    let rect = split.rect;
    let ratio = f32::from(split.ratio) / 1000.;
    match split.axis {
        Axis::SideBySide => handle
            .left(relative(rect.x as f32 + rect.width as f32 * ratio))
            .ml(-gap)
            .w(gap * 2.)
            .cursor_col_resize(),
        Axis::Stacked => handle
            .top(relative(rect.y as f32 + rect.height as f32 * ratio))
            .mt(-gap)
            .h(gap * 2.)
            .cursor_row_resize(),
    }
}

fn divider_line(t: &tokens::Theme, axis: Axis, is_dragged: bool) -> Div {
    let accent = t.colors.accent;
    div()
        .bg(match is_dragged {
            true => accent,
            false => gpui_kit::transparent_black(),
        })
        .group_hover("tile-divider", move |style| style.bg(accent))
        .when(axis == Axis::SideBySide, |line| {
            line.w(t.spacing.xxs / 2.).h_full()
        })
        .when(axis == Axis::Stacked, |line| {
            line.h(t.spacing.xxs / 2.).w_full()
        })
}
