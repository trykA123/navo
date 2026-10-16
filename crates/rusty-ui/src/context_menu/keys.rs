use gpui_kit::base::ObservedElement;
use gpui_kit::{App, Context, Div, InteractiveElement, KeyBinding, Stateful, Window};

use super::place::ROW;
use super::state::{ContextMenuEvent, Escape, Step};
use super::{CONTEXT, ContextMenu};

gpui_kit::actions!(
    context_menu,
    [
        MenuUp,
        MenuDown,
        MenuHome,
        MenuEnd,
        MenuPageUp,
        MenuPageDown,
        MenuEnter,
        MenuRight,
        MenuLeft,
        MenuEscape,
        MenuClassic,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let field = format!("{CONTEXT} > Input");
    for context in [CONTEXT, field.as_str()] {
        let context = Some(context);
        cx.bind_keys([
            KeyBinding::new("up", MenuUp, context),
            KeyBinding::new("down", MenuDown, context),
            KeyBinding::new("home", MenuHome, context),
            KeyBinding::new("end", MenuEnd, context),
            KeyBinding::new("pageup", MenuPageUp, context),
            KeyBinding::new("pagedown", MenuPageDown, context),
            KeyBinding::new("enter", MenuEnter, context),
            KeyBinding::new("right", MenuRight, context),
            KeyBinding::new("left", MenuLeft, context),
            KeyBinding::new("escape", MenuEscape, context),
            KeyBinding::new("shift-f10", MenuClassic, context),
        ]);
    }
}

pub(super) fn register(
    layer: ObservedElement<Stateful<Div>>,
    cx: &mut Context<ContextMenu>,
) -> ObservedElement<Stateful<Div>> {
    layer
        .on_action(cx.listener(ContextMenu::on_up))
        .on_action(cx.listener(ContextMenu::on_down))
        .on_action(cx.listener(ContextMenu::on_home))
        .on_action(cx.listener(ContextMenu::on_end))
        .on_action(cx.listener(ContextMenu::on_page_up))
        .on_action(cx.listener(ContextMenu::on_page_down))
        .on_action(cx.listener(ContextMenu::on_enter))
        .on_action(cx.listener(ContextMenu::on_right))
        .on_action(cx.listener(ContextMenu::on_left))
        .on_action(cx.listener(ContextMenu::on_escape))
        .on_action(cx.listener(ContextMenu::on_classic))
}

impl ContextMenu {
    fn on_up(&mut self, _: &MenuUp, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Previous, cx);
    }

    fn on_down(&mut self, _: &MenuDown, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Next, cx);
    }

    fn on_home(&mut self, _: &MenuHome, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::First, cx);
    }

    fn on_end(&mut self, _: &MenuEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Last, cx);
    }

    fn on_page_up(&mut self, _: &MenuPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::PageUp(self.page_rows()), cx);
    }

    fn on_page_down(&mut self, _: &MenuPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::PageDown(self.page_rows()), cx);
    }

    fn on_enter(&mut self, _: &MenuEnter, _: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.enter();
        self.finish(event, cx);
    }

    fn on_right(&mut self, _: &MenuRight, _: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.right();
        self.finish(event, cx);
    }

    fn on_left(&mut self, _: &MenuLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.state.left();
        self.finish(None, cx);
    }

    fn on_escape(&mut self, _: &MenuEscape, window: &mut Window, cx: &mut Context<Self>) {
        match self.state.escape() {
            Escape::ClosedSubmenu => self.finish(None, cx),
            Escape::ClearedQuery => self.set_query("", window, cx),
            Escape::Dismiss => self.finish(Some(ContextMenuEvent::Dismissed), cx),
        }
    }

    fn on_classic(&mut self, _: &MenuClassic, _: &mut Window, cx: &mut Context<Self>) {
        self.finish(Some(ContextMenuEvent::ShowClassic), cx);
    }

    fn step(&mut self, step: Step, cx: &mut Context<Self>) {
        self.hover = None;
        self.state.move_by(step);
        self.finish(None, cx);
    }

    fn page_rows(&self) -> usize {
        let frames = self.frames();
        let height = frames
            .get(self.state.active())
            .map_or(ROW, |frame| f32::from(frame.list_height));
        ((height / ROW).floor() as usize).max(1)
    }
}
