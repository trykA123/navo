use gpui_kit::{Action, Bounds, FocusHandle, Focusable, Stateful, point};
use rusty_core::shell_menu::{ItemId, ShellMenu};

use super::body::{BodyAct, MenuEntry, ViewerBody};
use super::menu::viewer_menu;
use super::parts::{PRESET_ACTIONS, PartSet};
use super::{
    Back, ChooseEncoding, KeepViewDefault, ResetView, RightDrawer, ShowLineEnds, ToggleCharts,
    ToggleInfo, ToggleLineNumbers, ToggleMemoryStrip, ToggleOpensInspector, TogglePreview,
    ToggleRowBars, ViewCustom, ViewMenu, ViewRich, ViewSimple,
};
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::settings::{Change, Field, PRESETS, Value, Values};
use crate::tokens;
use crate::window_view::{FerriteWindow, SaveReport};

pub(in crate::window_view) mod labels;
mod split;

use labels::{
    ENCODING, KEEP_LABEL, KEPT, MENU_TITLE, NO_PARTS, NOT_KEPT, NOT_SAVED, RESET_LABEL,
    empty_keep_note,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewOp {
    Toggle(Field),
    Preset(usize),
    Keep,
    Reset,
    Encoding,
}

impl ViewOp {
    fn keeps_menu_open(self) -> bool {
        matches!(self, Self::Toggle(_) | Self::Preset(_))
    }
}

pub(in crate::window_view) fn view_entries(
    set: &PartSet,
    shown: &Values,
    is_editing: bool,
    has_encoding: bool,
) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    if let Some(preset) = set.preset {
        let at = shown.get(preset).clone();
        for (index, (choice, action)) in PRESETS.iter().zip(PRESET_ACTIONS).enumerate() {
            let is_on = at == Value::Pick(index);
            entries.push(MenuEntry::Run(choice.label, action(), is_on));
        }
        entries.push(MenuEntry::Break);
    }
    for part in set.parts {
        if is_editing && part.field == Field::TextLineEnds {
            continue;
        }
        let is_on = shown.is_shown(set.preset, part.field);
        entries.push(MenuEntry::Run(
            part.field.spec().label,
            (part.action)(),
            is_on,
        ));
    }
    if has_encoding {
        entries.push(MenuEntry::Break);
        entries.push(MenuEntry::Run(ENCODING, Box::new(ChooseEncoding), false));
    }
    entries.push(MenuEntry::Break);
    entries.push(MenuEntry::Run(KEEP_LABEL, Box::new(KeepViewDefault), false));
    entries.push(MenuEntry::Run(RESET_LABEL, Box::new(ResetView), false));
    entries
}

pub fn view_menu(
    body: &dyn ViewerBody,
    shown: &Values,
    window: &Window,
    cx: &App,
) -> Option<ShellMenu> {
    let set = body.parts(cx)?;
    let focus = body.focus_handle(cx);
    let has_encoding = body.encoding(cx).is_some();
    let entries = view_entries(set, shown, body.is_editing(cx), has_encoding);
    Some(viewer_menu(&entries, &focus, window))
}

pub fn view_button() -> impl IntoElement {
    Button::new("viewer-view", "View")
        .variant(ButtonVariant::Ghost)
        .tooltip("View menu (Alt+V)")
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ViewMenu), cx))
}

fn op_of(set: &PartSet, name: &str) -> Option<ViewOp> {
    let part = set
        .parts
        .iter()
        .find(|part| (part.action)().name() == name)
        .map(|part| ViewOp::Toggle(part.field));
    let preset = || {
        PRESET_ACTIONS
            .iter()
            .position(|action| action().name() == name)
            .map(ViewOp::Preset)
    };
    part.or_else(preset).or_else(|| {
        [
            (KeepViewDefault.name(), ViewOp::Keep),
            (ResetView.name(), ViewOp::Reset),
            (ChooseEncoding.name(), ViewOp::Encoding),
        ]
        .into_iter()
        .find(|(action, _)| *action == name)
        .map(|(_, op)| op)
    })
}

fn on_view<A: Action>(
    root: Stateful<Div>,
    op: ViewOp,
    cx: &mut Context<FerriteWindow>,
) -> Stateful<Div> {
    root.on_action(cx.listener(move |view, _: &A, window, cx| view.run_view(op, window, cx)))
}

impl FerriteWindow {
    pub(super) fn view_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let root = root
            .on_action(cx.listener(|view, _: &ViewMenu, window, cx| {
                view.open_view_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ChooseEncoding, window, cx| {
                view.open_encoding_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ShowLineEnds, window, cx| {
                view.show_line_ends(window, cx);
            }))
            .on_action(cx.listener(|view, _: &TogglePreview, window, cx| {
                view.toggle_split(window, cx);
            }))
            .on_action(cx.listener(|view, _: &RightDrawer, window, cx| {
                view.right_drawer(window, cx);
            }))
            .on_action(cx.listener(|view, _: &Back, window, cx| view.close_drawer(window, cx)));
        let root = on_view::<ToggleLineNumbers>(root, ViewOp::Toggle(Field::TextLineNumbers), cx);
        let root = on_view::<ToggleInfo>(root, ViewOp::Toggle(Field::ImageDetails), cx);
        let root =
            on_view::<ToggleOpensInspector>(root, ViewOp::Toggle(Field::AutomotiveInspector), cx);
        let root =
            on_view::<ToggleMemoryStrip>(root, ViewOp::Toggle(Field::AutomotiveMemoryStrip), cx);
        let root = on_view::<ToggleCharts>(root, ViewOp::Toggle(Field::AutomotiveCharts), cx);
        let root = on_view::<ToggleRowBars>(root, ViewOp::Toggle(Field::AutomotiveRowBars), cx);
        let root = on_view::<ViewSimple>(root, ViewOp::Preset(0), cx);
        let root = on_view::<ViewRich>(root, ViewOp::Preset(1), cx);
        let root = on_view::<ViewCustom>(root, ViewOp::Preset(2), cx);
        let root = on_view::<KeepViewDefault>(root, ViewOp::Keep, cx);
        on_view::<ResetView>(root, ViewOp::Reset, cx)
    }

    fn body_parts(&self, cx: &App) -> Option<&'static PartSet> {
        self.viewer.as_ref()?.body.as_ref()?.parts(cx)
    }

    pub(in crate::window_view) fn show_parts(&self, cx: &mut App) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        let Some(body) = &viewer.body else {
            return;
        };
        if body.parts(cx).is_some() {
            body.show_parts(&viewer.parts.effective(&self.values), cx);
        }
    }

    fn run_view(&mut self, op: ViewOp, window: &mut Window, cx: &mut Context<Self>) {
        if op == ViewOp::Encoding {
            return self.open_encoding_menu(window, cx);
        }
        let Some(set) = self.body_parts(cx) else {
            return;
        };
        let values = self.values.clone();
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let kept = match op {
            ViewOp::Toggle(field) if set.has(field) => {
                viewer.parts.toggle(set, field, &values);
                Vec::new()
            }
            ViewOp::Toggle(_) => return,
            ViewOp::Preset(at) => {
                viewer.parts.choose(set, at, &values);
                Vec::new()
            }
            ViewOp::Reset => {
                viewer.parts.reset();
                Vec::new()
            }
            ViewOp::Encoding => return,
            ViewOp::Keep => {
                let kept = viewer.parts.keep(&values);
                let note = empty_keep_note(&viewer.parts);
                if kept.is_empty() {
                    viewer.say(note, cx);
                }
                kept
            }
        };
        for change in kept {
            self.change_setting(change, window, cx);
        }
        self.show_parts(cx);
        if matches!(
            op,
            ViewOp::Toggle(Field::MarkdownPreviewSplit) | ViewOp::Reset
        ) {
            self.leave_hidden_split(window, cx);
        }
        self.refresh_view_menu(window, cx);
        cx.notify();
    }

    pub(in crate::window_view) fn keep_settled(
        &mut self,
        report: &SaveReport,
        sent: &[Change],
        cx: &mut App,
    ) {
        self.preview_keep_settled(report, sent);
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let is_clean = report.failed.is_empty() && report.error.is_none();
        match viewer.parts.settle(sent, is_clean) {
            Some(true) => viewer.say(KEPT, cx),
            Some(false) => {
                let problem = report.error.as_deref().unwrap_or(NOT_SAVED);
                viewer.say(format!("{NOT_KEPT} {problem}"), cx);
            }
            None => {}
        }
    }

    fn show_line_ends(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) else {
            return;
        };
        let can_show = !body.is_editing(cx)
            && body
                .parts(cx)
                .is_some_and(|set| set.has(Field::TextLineEnds));
        if can_show {
            self.run_view(ViewOp::Toggle(Field::TextLineEnds), window, cx);
        } else {
            body.act(BodyAct::LineEnds, window, cx);
        }
    }

    fn right_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.body_parts(cx).and_then(|set| set.right_drawer) {
            self.run_view(ViewOp::Toggle(field), window, cx);
        }
    }

    fn close_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.viewer.as_ref().and_then(|viewer| {
            let set = viewer.body.as_ref()?.parts(cx)?;
            let field = set.right_drawer?;
            viewer
                .parts
                .is_shown(set, field, &self.values)
                .then_some(field)
        });
        match open {
            Some(field) => self.run_view(ViewOp::Toggle(field), window, cx),
            None => cx.propagate(),
        }
    }

    fn view_shell_menu(&self, window: &Window, cx: &App) -> Option<(ShellMenu, FocusHandle)> {
        let viewer = self.viewer.as_ref()?;
        let body = viewer.body.as_ref()?;
        let focus = body.focus_handle(cx);
        let shown = viewer.parts.effective(&self.values);
        let menu = view_menu(body.as_ref(), &shown, window, cx)?;
        Some((menu, focus))
    }

    fn open_view_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((menu, focus)) = self.view_shell_menu(window, cx) else {
            if let Some(viewer) = self.viewer.as_mut() {
                viewer.say(NO_PARTS, cx);
            }
            cx.notify();
            return;
        };
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let t = tokens::current(cx);
        let area = viewer.bounds.get();
        let anchor = point(
            area.right() - t.spacing.xl,
            area.top() + t.spacing.path_height,
        );
        let input = ContextMenuInput {
            menu,
            context: MENU_TITLE.into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(focus),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu.name_ferrite_group(MENU_TITLE, cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.view_menu_event(event, window, cx);
        });
        viewer.menu = Some((menu, events));
        viewer.is_view_menu = true;
        cx.notify();
    }

    fn view_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let op = match event {
            ContextMenuEvent::OpenSubmenu(_) => return,
            ContextMenuEvent::Chosen(ItemId::Action(name)) => {
                self.body_parts(cx).and_then(|set| op_of(set, name))
            }
            _ => None,
        };
        match op {
            Some(op) if op.keeps_menu_open() => {
                self.run_view(op, window, cx);
                self.focus_view_menu(window, cx);
            }
            Some(op) => {
                self.close_view_menu(cx);
                self.run_view(op, window, cx);
            }
            None => self.close_view_menu(cx),
        }
    }

    fn close_view_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.menu = None;
            viewer.is_view_menu = false;
        }
        cx.notify();
    }

    fn focus_view_menu(&self, window: &mut Window, cx: &mut App) {
        let menu = self
            .viewer
            .as_ref()
            .filter(|viewer| viewer.is_view_menu)
            .and_then(|viewer| viewer.menu.as_ref())
            .map(|(menu, _)| menu.clone());
        if let Some(menu) = menu {
            let focus = menu.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    fn refresh_view_menu(&self, window: &Window, cx: &mut App) {
        let open = self
            .viewer
            .as_ref()
            .filter(|viewer| viewer.is_view_menu)
            .and_then(|viewer| viewer.menu.as_ref())
            .map(|(menu, _)| menu.clone());
        let Some(menu) = open else {
            return;
        };
        if let Some((shell, _)) = self.view_shell_menu(window, cx) {
            menu.update(cx, |menu, cx| menu.set_menu(shell, cx));
        }
    }
}
