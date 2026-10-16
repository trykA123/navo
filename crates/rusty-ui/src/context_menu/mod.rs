mod icons;
mod keys;
mod motion;
mod panel;
mod place;
mod row;
mod state;

#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

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
use motion::{GROW, Grow, RootMotion};
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
    pub restore: Option<FocusHandle>,
}

pub struct ContextMenu {
    state: MenuState,
    context: SharedString,
    bounds: Bounds<Pixels>,
    field: Entity<TextFieldState>,
    scrolls: Vec<(Vec<usize>, ScrollHandle)>,
    icons: ShellIcons,
    hover: Option<Task<()>>,
    restore: Option<FocusHandle>,
    opened: Instant,
    origin: Point<Pixels>,
    grow: Option<Grow>,
    has_classic: bool,
    _field: Subscription,
    _release: Subscription,
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
        let release = cx.on_release(|menu: &mut Self, cx| {
            for image in menu.icons.drain() {
                cx.drop_image(image, None);
            }
        });
        let icons = ShellIcons::build(&input.menu);
        let state = MenuState::new(input.menu);
        let origin = place::root_frame(&state.levels()[0], input.anchor, input.bounds)
            .bounds
            .origin;
        let mut menu = Self {
            icons,
            state,
            opened: cx.background_executor().now(),
            origin,
            grow: None,
            has_classic: true,
            context: input.context,
            bounds: input.bounds,
            field,
            scrolls: Vec::new(),
            hover: None,
            restore: input.restore,
            _field: subscription,
            _release: release,
        };
        menu.sync_scrolls();
        menu
    }

    pub fn menu(&self) -> &ShellMenu {
        self.state.menu()
    }

    pub fn hide_classic(&mut self, cx: &mut Context<Self>) {
        self.has_classic = false;
        cx.notify();
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

    pub fn set_menu(&mut self, menu: ShellMenu, cx: &mut Context<Self>) {
        let stale: Vec<_> = self.icons.drain().collect();
        for image in stale {
            cx.drop_image(image, None);
        }
        self.icons = ShellIcons::build(&menu);
        let now = cx.background_executor().now();
        let from = self.root_list_height(now);
        let rows = self.state.levels()[0].rows.clone();
        self.state.set_menu(menu);
        self.grow = (!cx.reduce_motion()).then_some(Grow {
            from,
            rows,
            started: now,
        });
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
            for image in self.icons.refill(self.state.menu(), path) {
                cx.drop_image(image, None);
            }
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

    fn conclude(
        &mut self,
        event: Option<ContextMenuEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closes = matches!(
            event,
            Some(
                ContextMenuEvent::Chosen(_)
                    | ContextMenuEvent::Dismissed
                    | ContextMenuEvent::ShowClassic
            )
        );
        if let (true, Some(restore)) = (closes, &self.restore) {
            window.focus(restore, cx);
        }
        self.finish(event, cx);
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

    fn clicked(
        &mut self,
        (depth, row): (usize, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hover = None;
        let event = self.state.click(depth, row);
        self.conclude(event, window, cx);
    }

    fn retried(&mut self, depth: usize, cx: &mut Context<Self>) {
        let event = self.state.retry(depth);
        self.finish(event, cx);
    }

    fn dismiss_click(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.conclude(Some(ContextMenuEvent::Dismissed), window, cx);
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
            side: above.side,
        }
    }

    fn grow_done(&self, now: Instant) -> f32 {
        self.grow
            .as_ref()
            .map_or(1., |grow| motion::progress(grow.started, now, GROW))
    }

    fn root_list_height(&self, now: Instant) -> Pixels {
        let to = self
            .frames()
            .first()
            .map_or(px(0.), |frame| frame.list_height);
        match &self.grow {
            Some(grow) => grow.height(to, self.grow_done(now)),
            None => to,
        }
    }

    fn root_look<'a>(&'a self, frame: &mut Frame, now: Instant, cx: &App) -> RootMotion<'a> {
        let entered = motion::progress(self.opened, now, tokens::current(cx).motion.fast);
        let done = self.grow_done(now);
        if let Some(grow) = &self.grow {
            let height = grow.height(frame.list_height, done);
            frame.bounds.size.height += height - frame.list_height;
            frame.list_height = height;
        }
        RootMotion {
            rise: motion::rise(entered, cx.reduce_motion()),
            opacity: entered,
            fresh: self.grow.as_ref().map(|grow| (grow.rows.as_slice(), done)),
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
                _ => place::pinned_root_frame(level, self.origin, self.bounds),
            };
            frames.push(frame);
        }
        frames
    }
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let reduce_motion = cx.reduce_motion();
        let now = cx.background_executor().now();
        if reduce_motion || self.grow_done(now) >= 1. {
            self.grow = None;
        }
        if self.grow.is_some() || now.saturating_duration_since(self.opened) < t.motion.fast {
            window.request_animation_frame();
        }
        let panels: Vec<_> = self
            .frames()
            .into_iter()
            .enumerate()
            .map(|(depth, mut frame)| {
                let root = (depth == 0).then(|| self.root_look(&mut frame, now, cx));
                let look = PanelLook {
                    frame,
                    theme: &t,
                    reduce_motion,
                    root,
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
