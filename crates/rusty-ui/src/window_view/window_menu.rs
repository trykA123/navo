use gpui_kit::{Focusable, Subscription, point};
use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

use super::FerriteWindow;
use super::keys::OpenViewMenu;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::settings::{Change, Choice, DENSITIES, FIELDS, Field, HINTS, Value, Values};
use crate::tokens;

const TITLE: &str = "View";
const RESET_NAME: &str = "window.reset";
const RESET_LABEL: &str = "Reset to defaults";
const FILES_LABEL: &str = "Files";

const WINDOW: [Field; 6] = [
    Field::WindowTabsWithOne,
    Field::WindowPathBar,
    Field::WindowDriveButton,
    Field::WindowTitleSearch,
    Field::WindowFkeyBar,
    Field::WindowIcons,
];
const COLUMNS: [Field; 3] = [
    Field::WindowExtColumn,
    Field::WindowSizeColumn,
    Field::WindowDateColumn,
];
const FOOTER: [Field; 3] = [
    Field::WindowFreeSpace,
    Field::WindowMarkedTotals,
    Field::WindowContents,
];
const FILES: [Field; 3] = [Field::HiddenFiles, Field::ShowSystem, Field::FoldersFirst];
const CHOICES: [(Field, &str, &[Choice]); 2] = [
    (Field::Density, "Density", &DENSITIES),
    (Field::ModifierHints, "Key hints", &HINTS),
];

#[derive(Default)]
pub(super) struct WindowMenu(Option<(Entity<ContextMenu>, Subscription)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Toggle(Field),
    Choose(Field, usize),
    Reset,
}

impl Op {
    fn keeps_menu_open(self) -> bool {
        matches!(self, Self::Toggle(_))
    }
}

fn is_on(values: &Values, field: Field) -> bool {
    match field {
        Field::HiddenFiles => values.show_hidden(),
        field => values.flag(field),
    }
}

fn heading(name: &'static str) -> MenuItem {
    MenuItem {
        enabled: false,
        ..MenuItem::new(ItemId::Action(name), name, Group::Ferrite)
    }
}

fn toggle(values: &Values, field: Field) -> MenuItem {
    let spec = field.spec();
    MenuItem {
        checked: is_on(values, field),
        ..MenuItem::new(ItemId::Action(spec.key), spec.label, Group::Ferrite)
    }
}

fn choice_menu(
    values: &Values,
    (field, label, choices): (Field, &'static str, &[Choice]),
) -> MenuItem {
    let current = values.get(field);
    let children = choices
        .iter()
        .enumerate()
        .map(|(at, choice)| MenuItem {
            checked: *current == Value::Pick(at),
            ..MenuItem::new(ItemId::Action(choice.toml), choice.label, Group::Ferrite)
        })
        .collect();
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..MenuItem::new(ItemId::Action(label), label, Group::Ferrite)
    }
}

fn files_menu(values: &Values) -> MenuItem {
    let children = FILES.iter().map(|field| toggle(values, *field)).collect();
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..MenuItem::new(ItemId::Action(FILES_LABEL), FILES_LABEL, Group::Ferrite)
    }
}

fn section(values: &Values, name: &'static str, fields: &[Field]) -> Vec<MenuItem> {
    let rows = fields.iter().map(|field| toggle(values, *field));
    std::iter::once(heading(name)).chain(rows).collect()
}

pub fn window_view_menu(values: &Values) -> ShellMenu {
    let separator = || MenuItem::separator(Group::Ferrite);
    let mut items = section(values, "Window", &WINDOW);
    items.extend(section(values, "Columns", &COLUMNS));
    items.extend(section(values, "Pane footer", &FOOTER));
    items.push(separator());
    items.push(files_menu(values));
    items.extend(CHOICES.iter().map(|choice| choice_menu(values, *choice)));
    items.push(separator());
    items.push(MenuItem::new(
        ItemId::Action(RESET_NAME),
        RESET_LABEL,
        Group::Ferrite,
    ));
    ShellMenu { items }
}

fn op_of(name: &str) -> Option<Op> {
    if name == RESET_NAME {
        return Some(Op::Reset);
    }
    let toggles = [&WINDOW[..], &COLUMNS, &FOOTER, &FILES].concat();
    let toggled = toggles.iter().find(|field| field.spec().key == name);
    let chosen = || {
        CHOICES.iter().find_map(|(field, _, choices)| {
            let at = choices.iter().position(|choice| choice.toml == name)?;
            Some(Op::Choose(*field, at))
        })
    };
    toggled.map(|field| Op::Toggle(*field)).or_else(chosen)
}

impl FerriteWindow {
    pub(super) fn window_menu_view(&self) -> Option<Entity<ContextMenu>> {
        Some(self.window_menu.0.as_ref()?.0.clone())
    }

    pub(super) fn view_menus(&self) -> Vec<Entity<ContextMenu>> {
        let preview = self.preview_menu_view();
        self.window_menu_view().into_iter().chain(preview).collect()
    }

    pub(super) fn open_window_menu_on(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &OpenViewMenu, window, cx| {
            view.open_window_menu(window, cx);
        }))
    }

    pub(super) fn can_open_window_menu(&self) -> bool {
        self.viewer.is_none() && self.prompt.is_none() && self.browser.pending().is_none()
    }

    fn open_window_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_open_window_menu() || self.window_menu.0.is_some() {
            return;
        }
        let spacing = tokens::current(cx).spacing;
        let viewport = window.viewport_size();
        let pane = self.browser.state().focused_pane();
        let focus = self.handle(pane, cx);
        let input = ContextMenuInput {
            menu: window_view_menu(&self.values),
            context: TITLE.into(),
            anchor: point(viewport.width - spacing.xl, spacing.header_height),
            bounds: Bounds::new(Point::default(), viewport),
            restore: Some(focus),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu.name_ferrite_group(TITLE, cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.window_menu_event(event, window, cx);
        });
        self.window_menu = WindowMenu(Some((menu, events)));
        cx.notify();
    }

    fn window_menu_event(
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
                self.run_window_op(op, window, cx);
                self.focus_window_menu(window, cx);
            }
            Some(op) => {
                self.close_window_menu(cx);
                self.run_window_op(op, window, cx);
            }
            None => self.close_window_menu(cx),
        }
    }

    fn run_window_op(&mut self, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let changes = match op {
            Op::Toggle(Field::HiddenFiles) => vec![Change::show_hidden(!self.values.show_hidden())],
            Op::Toggle(field) => vec![Change {
                field,
                value: Value::Flag(!self.values.flag(field)),
            }],
            Op::Choose(field, at) => vec![Change {
                field,
                value: Value::Pick(at),
            }],
            Op::Reset => FIELDS
                .iter()
                .map(|spec| spec.field)
                .filter(|field| field.is_window_part() && self.values.is_changed(*field))
                .map(Change::reset)
                .collect(),
        };
        for change in changes {
            self.change_setting(change, window, cx);
        }
    }

    fn close_window_menu(&mut self, cx: &mut Context<Self>) {
        self.window_menu = WindowMenu::default();
        cx.notify();
    }

    fn focus_window_menu(&self, window: &mut Window, cx: &mut App) {
        if let Some((menu, _)) = &self.window_menu.0 {
            let focus = menu.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    pub(super) fn refresh_window_menu(&self, cx: &mut App) {
        if let Some((menu, _)) = &self.window_menu.0 {
            let shell = window_view_menu(&self.values);
            menu.update(cx, |menu, cx| menu.set_menu(shell, cx));
        }
    }
}
