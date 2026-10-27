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
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, NavigationDirection,
    ParentElement, Pixels, Point, Render, ScrollHandle, SharedString, Styled, Subscription, Task,
    Window, anchored, deferred, div, px,
};
use rusty_core::shell_menu::{ShellMenu, Submenu};

pub use keys::bind_keys;
pub(crate) use keys::{MenuDown, MenuEnter, MenuEscape, MenuUp};
pub use state::ContextMenuEvent;

use crate::primitives::{TextFieldState, text_field_state};
use crate::tokens;
use icons::ShellIcons;
use panel::PanelLook;
use place::{Frame, Panel};
use state::{MenuState, PageStatus};

const CONTEXT: &str = "ContextMenu";
const ROOT: &str = "context-menu";
const PLACEHOLDER: &str = "Type to filter commands";
const LOADING_DELAY: Duration = Duration::from_millis(120);

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
    scroll: ScrollHandle,
    icons: ShellIcons,
    restore: Option<FocusHandle>,
    opened: Instant,
    origin: Point<Pixels>,
    width: Pixels,
    shown_page: Vec<usize>,
    page_since: Instant,
    loading: Option<Task<()>>,
    has_classic: bool,
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
        let icons = ShellIcons::build(&input.menu, cx);
        let state = MenuState::new(input.menu);
        let width = px(place::page_width(state.rows(), state.menu()));
        let panel = Panel {
            origin: input.anchor,
            width,
            list: place::list_height(state.rows(), state.status()),
            has_back: false,
        };
        let now = cx.background_executor().now();
        let mut menu = Self {
            icons,
            origin: place::opening_origin(panel, input.anchor, input.bounds),
            width,
            state,
            opened: now,
            shown_page: Vec::new(),
            page_since: now,
            loading: None,
            has_classic: true,
            context: input.context,
            bounds: input.bounds,
            field,
            scroll: ScrollHandle::new(),
            restore: input.restore,
            _field: subscription,
        };
        menu.settle(cx);
        crate::perf::ready(crate::perf::Latency::ContextMenu);
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
        self.state.set_query(query);
        self.finish(None, window, cx);
    }

    pub fn set_menu(&mut self, menu: ShellMenu, cx: &mut Context<Self>) {
        self.icons = ShellIcons::build(&menu, cx);
        self.state.set_menu(menu);
        self.settle(cx);
        cx.notify();
    }

    pub fn set_submenu(
        &mut self,
        path: &[usize],
        submenu: Submenu,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.state.set_submenu(path, submenu);
        if changed {
            self.icons.refill(self.state.menu(), path, cx);
            self.settle(cx);
            cx.notify();
        }
        changed
    }

    pub fn open_submenu(&mut self, path: &[usize], window: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.drill(path.to_vec());
        self.finish(event, window, cx);
    }

    fn field_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.field.read(cx).value();
        self.state.set_query(&query);
        self.settle(cx);
        cx.notify();
    }

    fn finish(
        &mut self,
        event: Option<ContextMenuEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.state.query().to_owned();
        if self.field.read(cx).value() != query {
            self.field
                .update(cx, |field, cx| field.set_value(query, window, cx));
        }
        self.settle(cx);
        if let Some(event) = event {
            cx.emit(event);
        }
        cx.notify();
    }

    fn settle(&mut self, cx: &mut Context<Self>) {
        if self.state.page() != self.shown_page {
            self.turn_page(cx);
        }
        if !self.state.is_filtering() {
            let wanted = px(place::page_width(self.state.rows(), self.state.menu()));
            self.width = self.width.max(wanted);
        }
        let offset = usize::from(!self.state.page().is_empty());
        if let Some(index) = self.state.selection().index() {
            self.scroll.scroll_to_item(index.saturating_sub(offset));
        }
    }

    fn turn_page(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.shown_page = self.state.page().to_vec();
        self.scroll = ScrollHandle::new();
        self.page_since = now;
        self.loading = (self.state.status() == PageStatus::Loading).then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(LOADING_DELAY).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
        });
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
        self.finish(event, window, cx);
    }

    fn hovered(&mut self, row: usize, cx: &mut Context<Self>) {
        if self.state.hover(row) {
            cx.notify();
        }
    }

    fn clicked(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.click(row);
        self.conclude(event, window, cx);
    }

    fn retried(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.retry();
        self.finish(event, window, cx);
    }

    fn went_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_query("");
        self.state.back();
        self.finish(None, window, cx);
    }

    fn back_click(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.went_back(window, cx);
    }

    fn dismiss_click(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.conclude(Some(ContextMenuEvent::Dismissed), window, cx);
    }

    fn frame(&self) -> Frame {
        let panel = Panel {
            origin: self.origin,
            width: self.width,
            list: place::list_height(self.state.rows(), self.state.status()),
            has_back: !self.state.page().is_empty(),
        };
        place::panel_frame(panel, self.bounds)
    }

    fn is_loading_shown(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.page_since) >= LOADING_DELAY
    }

    fn opacity(&self, now: Instant, cx: &App) -> f32 {
        motion::progress(self.opened, now, tokens::current(cx).motion.fade)
    }
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let now = cx.background_executor().now();
        let opacity = self.opacity(now, cx);
        let is_fading = opacity < 1.;
        if is_fading {
            window.request_animation_frame();
        }
        crate::perf::note(crate::perf::UiFlag::ContextMenu);
        crate::perf::animating(crate::perf::Latency::MenuOpen, is_fading);
        let look = PanelLook {
            frame: self.frame(),
            theme: &t,
            opacity,
            shows_loading: self.is_loading_shown(now),
        };
        let panel = self.panel(look, cx);
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
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(Self::back_click),
            )
            .child(panel);
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
