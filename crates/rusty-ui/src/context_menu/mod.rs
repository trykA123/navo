mod aim;
mod flyout;
mod hover;
mod icons;
mod keys;
mod levels;
mod motion;
mod panel;
mod place;
mod row;
mod state;

#[cfg(test)]
mod tests;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{
    AnchoredPositionMode, AnyElement, App, Bounds, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, NavigationDirection,
    ParentElement, Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, Window, anchored, deferred, div, px,
};
use rusty_core::shell_menu::{ShellMenu, Submenu};
use rusty_l10n::tr;

pub use keys::bind_keys;
pub(crate) use keys::{MenuDown, MenuEnter, MenuEscape, MenuUp};
pub use state::ContextMenuEvent;

use crate::primitives::{TextFieldState, text_field_state};
use crate::tokens;
use icons::ShellIcons;
use levels::LevelView;
use panel::PanelLook;
use place::Panel;
use state::{MenuState, Spot};

const CONTEXT: &str = "ContextMenu";
const ROOT: &str = "context-menu";

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
    icons: ShellIcons,
    restore: Option<FocusHandle>,
    origin: Point<Pixels>,
    width: Pixels,
    views: Vec<LevelView>,
    intent: Option<Task<()>>,
    pointer: Option<Point<Pixels>>,
    settled: Option<Spot>,
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
        let field = text_field_state(tr("context-menu-type-to-filter"), window, cx);
        let subscription = cx.subscribe(&field, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.field_changed(cx);
            }
        });
        window.focus(&field.focus_handle(cx), cx);
        let icons = ShellIcons::build(&input.menu, cx);
        let state = MenuState::new(input.menu);
        let root = state.root();
        let width = px(place::page_width(root.rows(), state.menu()).max(place::MIN_WIDTH));
        let panel = Panel {
            origin: input.anchor,
            width,
            list: place::list_height(root.rows(), root.status()),
        };
        let mut menu = Self {
            icons,
            origin: place::opening_origin(panel, input.anchor, input.bounds),
            width,
            state,
            views: Vec::new(),
            intent: None,
            pointer: None,
            settled: None,
            has_classic: true,
            context: input.context,
            bounds: input.bounds,
            field,
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

    /// Opens the flyouts along `path` and focuses the deepest one.
    pub fn open_submenu(&mut self, path: &[usize], window: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.open_path(path);
        self.finish(event, window, cx);
    }

    fn field_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.field.read(cx).value();
        self.state.set_query(&query);
        self.intent = None;
        self.settle(cx);
        cx.notify();
    }

    fn finish(
        &mut self,
        event: Option<ContextMenuEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.intent = None;
        self.settled = None;
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
        self.sync_levels(cx);
        if !self.state.is_filtering() {
            self.width = self.width.max(px(self.root_width()));
        }
        for (view, level) in self.views.iter_mut().zip(self.state.levels()) {
            let index = level.selection().index();
            if view.selected == index {
                continue;
            }
            view.selected = index;
            if let Some(index) = index {
                view.scroll.scroll_to_item(index);
            }
        }
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

    fn retried(&mut self, level: usize, window: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.retry(level);
        self.finish(event, window, cx);
    }

    fn back_click(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.close_innermost() {
            self.finish(None, window, cx);
        }
    }

    fn dismiss_click(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.conclude(Some(ContextMenuEvent::Dismissed), window, cx);
    }

    fn panels(
        &self,
        t: &tokens::Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let now = cx.background_executor().now();
        let fade = t.motion.fade;
        let reduce = cx.reduce_motion();
        let mut is_moving = false;
        let mut panels = Vec::new();
        for (level, (placed, view)) in self.frames().into_iter().zip(&self.views).enumerate() {
            let opacity = motion::progress(view.opened, now, fade);
            let loading = flyout::bar_for(view, now, fade, reduce);
            if level == 0 {
                crate::perf::animating(crate::perf::Latency::MenuOpen, opacity < 1.);
            }
            is_moving |= opacity < 1. || loading.is_some_and(flyout::Bar::is_moving);
            let look = PanelLook {
                level,
                frame: placed.frame,
                theme: t,
                opacity,
                loading,
            };
            panels.push(if level == 0 {
                self.panel(look, cx)
            } else {
                self.flyout(look, cx)
            });
        }
        if is_moving {
            window.request_animation_frame();
        }
        panels
    }
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        crate::perf::note(crate::perf::UiFlag::ContextMenu);
        let panels = self.panels(&t, window, cx);
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
            .on_hover(cx.listener(|this, hovered: &bool, window, _| {
                if !*hovered {
                    this.pointer_left(window.mouse_position());
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::dismiss_click))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::dismiss_click))
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(Self::back_click),
            )
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
