mod icons;
mod keys;
mod panel;
mod place;
mod row;
mod state;

#[cfg(test)]
mod tests;

use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{
    AnchoredPositionMode, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Point,
    Render, ScrollHandle, SharedString, Styled, Subscription, Task, Window, anchored, deferred,
    div, px,
};
use rusty_core::shell_menu::{ShellMenu, Submenu};

pub use keys::bind_keys;
pub use state::ContextMenuEvent;

use crate::primitives::{TextFieldState, text_field_state};
use crate::tokens;
use icons::ShellIcons;
use panel::PanelLook;
use place::{BORDER, CONTEXT_LINE, FIELD, Frame, PAD, SubmenuSpot};
use state::MenuState;

const CONTEXT: &str = "ContextMenu";
const ROOT: &str = "context-menu";
const PLACEHOLDER: &str = "Type to filter commands";
const HOVER_DELAY: Duration = Duration::from_millis(150);

pub struct ContextMenuInput {
    pub menu: ShellMenu,
    pub context: SharedString,
    pub anchor: Point<Pixels>,
    pub bounds: Bounds<Pixels>,
}

pub struct ContextMenu {
    state: MenuState,
    context: SharedString,
    anchor: Point<Pixels>,
    bounds: Bounds<Pixels>,
    field: Entity<TextFieldState>,
    scrolls: Vec<(Vec<usize>, ScrollHandle)>,
    icons: ShellIcons,
    hover: Option<Task<()>>,
    _field: Subscription,
}

impl EventEmitter<ContextMenuEvent> for ContextMenu {}

impl Focusable for ContextMenu {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl ContextMenu {
    pub fn new(input: ContextMenuInput, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = text_field_state(PLACEHOLDER, window, cx);
        let subscription = cx.subscribe(&field, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.field_changed(cx);
            }
        });
        window.focus(&field.focus_handle(cx), cx);
        let mut menu = Self {
            icons: ShellIcons::build(&input.menu),
            state: MenuState::new(input.menu),
            context: input.context,
            anchor: input.anchor,
            bounds: input.bounds,
            field,
            scrolls: Vec::new(),
            hover: None,
            _field: subscription,
        };
        menu.sync_scrolls();
        menu
    }

    pub fn query(&self, cx: &App) -> SharedString {
        self.field.read(cx).value()
    }

    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = query.to_owned();
        self.field
            .update(cx, |field, cx| field.set_value(text, window, cx));
        self.state.set_query(query);
        self.finish(None, cx);
    }

    pub fn set_submenu(
        &mut self,
        path: &[usize],
        submenu: Submenu,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.state.set_submenu(path, submenu);
        if changed {
            self.icons = ShellIcons::build(self.state.menu());
            self.finish(None, cx);
        }
        changed
    }

    pub fn open_submenu(&mut self, path: &[usize], cx: &mut Context<Self>) {
        let event = self.state.open_path(path);
        self.finish(event, cx);
    }

    fn field_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.field.read(cx).value();
        self.hover = None;
        self.state.set_query(&query);
        self.finish(None, cx);
    }

    fn finish(&mut self, event: Option<ContextMenuEvent>, cx: &mut Context<Self>) {
        self.sync_scrolls();
        if let Some(event) = event {
            cx.emit(event);
        }
        cx.notify();
    }

    fn hovered(&mut self, depth: usize, row: usize, cx: &mut Context<Self>) {
        if !self.state.hover(depth, row) {
            return;
        }
        self.hover = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.hover = None;
                let event = this.state.settle(depth, row);
                this.finish(event, cx);
            });
        }));
        cx.notify();
    }

    fn clicked(&mut self, depth: usize, row: usize, cx: &mut Context<Self>) {
        self.hover = None;
        let event = self.state.click(depth, row);
        self.finish(event, cx);
    }

    fn retried(&mut self, depth: usize, cx: &mut Context<Self>) {
        let event = self.state.retry(depth);
        self.finish(event, cx);
    }

    fn dismiss_click(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.finish(Some(ContextMenuEvent::Dismissed), cx);
    }

    fn sync_scrolls(&mut self) {
        let levels = self.state.levels();
        self.scrolls.truncate(levels.len());
        for (depth, level) in levels.iter().enumerate() {
            if self
                .scrolls
                .get(depth)
                .is_none_or(|(path, _)| *path != level.path)
            {
                self.scrolls.truncate(depth);
                self.scrolls.push((level.path.clone(), ScrollHandle::new()));
            }
            if let Some(index) = level.selection.index() {
                self.scrolls[depth].1.scroll_to_item(index);
            }
        }
    }

    fn scroll(&self, depth: usize) -> ScrollHandle {
        self.scrolls
            .get(depth)
            .map(|(_, scroll)| scroll.clone())
            .unwrap_or_default()
    }

    fn submenu_spot(&self, parent: usize, above: &Frame) -> SubmenuSpot {
        let chrome = if parent == 0 {
            FIELD + CONTEXT_LINE
        } else {
            0.
        };
        SubmenuSpot {
            parent: above.bounds,
            list_top: above.bounds.origin.y + px(BORDER + PAD + chrome),
            scroll: self.scroll(parent).offset().y,
        }
    }

    fn frames(&self) -> Vec<Frame> {
        let levels = self.state.levels();
        let mut frames: Vec<Frame> = Vec::with_capacity(levels.len());
        for (depth, level) in levels.iter().enumerate() {
            let frame = match (depth.checked_sub(1), frames.last()) {
                (Some(parent), Some(above)) => {
                    let spot = self.submenu_spot(parent, above);
                    place::submenu_frame(level, &levels[parent].rows, spot, self.bounds)
                }
                _ => place::root_frame(level, self.anchor, self.bounds),
            };
            frames.push(frame);
        }
        frames
    }
}

impl Render for ContextMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let reduce_motion = cx.reduce_motion();
        let panels: Vec<_> = self
            .frames()
            .into_iter()
            .enumerate()
            .map(|(depth, frame)| {
                let look = PanelLook {
                    frame,
                    theme: &t,
                    reduce_motion,
                };
                self.panel(depth, look, cx)
            })
            .collect();
        let area = self.bounds;
        let layer = div()
            .id(ROOT)
            .key_context(CONTEXT)
            .test_support()
            .occlude()
            .relative()
            .w(area.size.width)
            .h(area.size.height)
            .font_family(t.typography.ui_font.clone())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::dismiss_click))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::dismiss_click))
            .children(panels);
        let layer = keys::register(layer, cx);
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(area.origin)
                .child(layer),
        )
        .with_priority(2)
    }
}
