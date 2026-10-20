use gpui_kit::{Action, Focusable, Stateful, Subscription, WeakEntity, point};
use rusty_core::shell_menu::{ItemId, ShellMenu};

use super::browse::{ToggleLargestFiles, ToggleTypeBars, ViewMenu};
use super::summary::Shows;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::settings::{Change, Field};
use crate::tokens;
use crate::window_view::viewer::parts::{Part, PartSet, ViewParts};
use crate::window_view::viewer::view_menu::labels::{
    KEPT, MENU_TITLE, NOT_KEPT, NOT_SAVED, empty_keep_note,
};
use crate::window_view::viewer::view_menu::view_entries;
use crate::window_view::viewer::{KeepViewDefault, ResetView, viewer_menu};
use crate::window_view::{FerriteWindow, SaveReport};

const PARTS: &[Part] = &[
    Part {
        field: Field::PreviewTypeBars,
        action: || Box::new(ToggleTypeBars),
    },
    Part {
        field: Field::PreviewLargestFiles,
        action: || Box::new(ToggleLargestFiles),
    },
];

pub(super) const PREVIEW: PartSet = PartSet {
    preset: None,
    parts: PARTS,
    family: PARTS,
    right_drawer: None,
};

#[derive(Default)]
pub(in crate::window_view) struct PreviewView {
    parts: ViewParts,
    menu: Option<(Entity<ContextMenu>, Subscription)>,
    note: Option<SharedString>,
}

impl PreviewView {
    pub(super) fn note(&self) -> Option<SharedString> {
        self.note.clone()
    }

    pub(super) fn reset(&mut self) {
        self.parts.reset();
        self.menu = None;
        self.note = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Toggle(Field),
    Keep,
    Reset,
}

impl Op {
    fn keeps_menu_open(self) -> bool {
        matches!(self, Self::Toggle(_))
    }
}

fn op_of(name: &str) -> Option<Op> {
    let part = PARTS
        .iter()
        .find(|part| (part.action)().name() == name)
        .map(|part| Op::Toggle(part.field));
    part.or_else(|| {
        [
            (KeepViewDefault.name(), Op::Keep),
            (ResetView.name(), Op::Reset),
        ]
        .into_iter()
        .find(|(action, _)| *action == name)
        .map(|(_, op)| op)
    })
}

pub(super) fn view_button(this: WeakEntity<FerriteWindow>) -> Button {
    Button::new("preview-view", "View")
        .variant(ButtonVariant::Ghost)
        .tooltip("View menu (Alt+V)")
        .on_click(move |_, window, cx| {
            let _ = this.update(cx, |view, cx| view.open_preview_menu(window, cx));
        })
}

impl FerriteWindow {
    pub(super) fn view_menu_listener(
        cx: &mut Context<Self>,
    ) -> impl Fn(&ViewMenu, &mut Window, &mut App) + 'static {
        cx.listener(|this, _: &ViewMenu, window, cx| this.open_preview_menu(window, cx))
    }

    pub(super) fn preview_shown(&self) -> Shows {
        Shows {
            type_bars: self.preview_shows(Field::PreviewTypeBars),
            largest: self.preview_shows(Field::PreviewLargestFiles),
        }
    }

    pub(super) fn part_actions(frame: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        frame
            .on_action(cx.listener(|this, _: &ToggleTypeBars, window, cx| {
                this.run_preview_op(Op::Toggle(Field::PreviewTypeBars), window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleLargestFiles, window, cx| {
                this.run_preview_op(Op::Toggle(Field::PreviewLargestFiles), window, cx);
            }))
    }

    pub(in crate::window_view) fn preview_shows(&self, field: Field) -> bool {
        self.preview
            .view
            .parts
            .is_shown(&PREVIEW, field, &self.values)
    }

    pub(in crate::window_view) fn preview_menu_view(&self) -> Option<Entity<ContextMenu>> {
        Some(self.preview.view.menu.as_ref()?.0.clone())
    }

    pub(in crate::window_view) fn apply_preview_parts(&mut self) {
        let is_shown = self.preview_shows(Field::PreviewLargestFiles);
        if let Some(folder) = self.preview.folder_mut() {
            folder.show_largest(is_shown);
        }
    }

    fn preview_shell_menu(&self, window: &Window) -> ShellMenu {
        let shown = self.preview.view.parts.effective(&self.values);
        let entries = view_entries(&PREVIEW, &shown, false, false);
        viewer_menu(&entries, &self.preview.browse, window)
    }

    pub(super) fn open_preview_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview.folder().is_none() || self.preview.view.menu.is_some() {
            return;
        }
        let spacing = tokens::current(cx).spacing;
        let viewport = window.viewport_size();
        let restore = if self.preview.is_browsing(window, cx) {
            self.preview.browse.clone()
        } else {
            let pane = self.browser.state().focused_pane();
            self.handle(pane, cx)
        };
        let input = ContextMenuInput {
            menu: self.preview_shell_menu(window),
            context: MENU_TITLE.into(),
            anchor: point(
                viewport.width - spacing.xl,
                spacing.header_height * 2. + spacing.path_height,
            ),
            bounds: Bounds::new(Point::default(), viewport),
            restore: Some(restore),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu.name_ferrite_group(MENU_TITLE, cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.preview_menu_event(event, window, cx);
        });
        self.preview.view.menu = Some((menu, events));
        cx.notify();
    }

    fn preview_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let op = match event {
            ContextMenuEvent::OpenSubmenu(_) => return,
            ContextMenuEvent::Chosen(ItemId::Action(name)) => op_of(name),
            _ => None,
        };
        match op {
            Some(op) if op.keeps_menu_open() => {
                self.run_preview_op(op, window, cx);
                self.focus_preview_menu(window, cx);
            }
            Some(op) => {
                self.close_preview_menu(cx);
                self.run_preview_op(op, window, cx);
            }
            None => self.close_preview_menu(cx),
        }
    }

    fn run_preview_op(&mut self, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let values = self.values.clone();
        let kept = match op {
            Op::Toggle(field) => {
                self.preview.view.parts.toggle(&PREVIEW, field, &values);
                self.preview.view.note = None;
                Vec::new()
            }
            Op::Reset => {
                self.preview.view.parts.reset();
                self.preview.view.note = None;
                Vec::new()
            }
            Op::Keep => {
                let kept = self.preview.view.parts.keep(&values);
                self.preview.view.note = Some(empty_keep_note(&self.preview.view.parts).into());
                kept
            }
        };
        for change in kept {
            self.change_setting(change, window, cx);
        }
        self.apply_preview_parts();
        self.refresh_preview_menu(window, cx);
        cx.notify();
    }

    fn close_preview_menu(&mut self, cx: &mut Context<Self>) {
        self.preview.view.menu = None;
        cx.notify();
    }

    fn focus_preview_menu(&self, window: &mut Window, cx: &mut App) {
        if let Some((menu, _)) = &self.preview.view.menu {
            let focus = menu.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    fn refresh_preview_menu(&self, window: &Window, cx: &mut App) {
        if let Some((menu, _)) = &self.preview.view.menu {
            let shell = self.preview_shell_menu(window);
            menu.update(cx, |menu, cx| menu.set_menu(shell, cx));
        }
    }

    pub(in crate::window_view) fn preview_keep_settled(
        &mut self,
        report: &SaveReport,
        sent: &[Change],
    ) {
        let is_clean = report.failed.is_empty() && report.error.is_none();
        let view = &mut self.preview.view;
        view.note = match view.parts.settle(sent, is_clean) {
            Some(true) => Some(KEPT.into()),
            Some(false) => {
                let problem = report.error.as_deref().unwrap_or(NOT_SAVED);
                Some(format!("{NOT_KEPT} {problem}").into())
            }
            None => view.note.take(),
        };
    }
}
