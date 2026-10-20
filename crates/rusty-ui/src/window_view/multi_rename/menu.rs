use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, Focusable, Subscription};
use rusty_core::rename::{ExtCase, FieldRegistry, NameCase, RenameRules};
use rusty_l10n::{t, tr};

use super::FerriteWindow;
use crate::browser::RenamePresetList;
use crate::context_menu::{MenuDown, MenuEnter, MenuEscape, MenuUp};
use crate::multi_rename::keys::ToggleRow;
use crate::multi_rename::view::fields::{
    FieldMenu, Fields, Mask, MenuRow, MenuSlot, ext_case_text, menu_tokens, name_case_text,
};
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};

const NAME_CASES: [NameCase; 5] = [
    NameCase::Unchanged,
    NameCase::Lower,
    NameCase::Upper,
    NameCase::FirstLetter,
    NameCase::Title,
];
const EXT_CASES: [ExtCase; 3] = [ExtCase::Unchanged, ExtCase::Lower, ExtCase::Upper];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Kind {
    Tokens(Mask),
    NameCase,
    ExtCase,
    Presets,
    SavePreset,
    DeletePreset(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Choice {
    Insert(Mask, String),
    NameCase(NameCase),
    ExtCase(ExtCase),
    Load(String),
    Save(String),
    Delete(String),
}

fn plain_row(text: String) -> MenuRow {
    MenuRow {
        text: text.into(),
        is_current: false,
    }
}

pub(super) struct Menu {
    pub kind: Kind,
    pub selected: usize,
    pub filter: Option<Entity<TextFieldState>>,
    pub focus: FocusHandle,
    pub presets: RenamePresetList,
    _filter_changed: Option<Subscription>,
}

impl Menu {
    fn query(&self, cx: &App) -> String {
        self.filter
            .as_ref()
            .map(|filter| filter.read(cx).value().trim().to_owned())
            .unwrap_or_default()
    }

    fn choices(
        &self,
        rules: &RenameRules,
        registry: &FieldRegistry,
        cx: &App,
    ) -> Vec<(MenuRow, Choice)> {
        let row = |text: String, is_current: bool| MenuRow {
            text: text.into(),
            is_current,
        };
        match &self.kind {
            Kind::Tokens(mask) => menu_tokens(&self.query(cx), registry)
                .into_iter()
                .map(|token| (row(token.text, false), Choice::Insert(*mask, token.insert)))
                .collect(),
            Kind::NameCase => NAME_CASES
                .into_iter()
                .map(|case| {
                    (
                        row(name_case_text(case), case == rules.name_case),
                        Choice::NameCase(case),
                    )
                })
                .collect(),
            Kind::ExtCase => EXT_CASES
                .into_iter()
                .map(|case| {
                    (
                        row(ext_case_text(case), case == rules.ext_case),
                        Choice::ExtCase(case),
                    )
                })
                .collect(),
            Kind::Presets => self.preset_choices(),
            Kind::SavePreset => self.save_choices(cx),
            Kind::DeletePreset(name) => vec![(
                row(t!("rename-presets-delete", name = name.clone()), false),
                Choice::Delete(name.clone()),
            )],
        }
    }

    fn preset_choices(&self) -> Vec<(MenuRow, Choice)> {
        let last = self.presets.last_used.clone().map(|rules| {
            let text = tr("rename-presets-last-used");
            (plain_row(text), Choice::Load(rules))
        });
        let named = self.presets.named.iter().map(|preset| {
            let row = plain_row(preset.name.clone());
            (row, Choice::Load(preset.rules.clone()))
        });
        last.into_iter().chain(named).collect()
    }

    fn save_choices(&self, cx: &App) -> Vec<(MenuRow, Choice)> {
        let name = self.query(cx);
        if name.is_empty() {
            return Vec::new();
        }
        let exists = self.presets.named.iter().any(|preset| preset.name == name);
        let text = if exists {
            t!("rename-presets-replace", name = name.clone())
        } else {
            tr("rename-presets-save")
        };
        vec![(plain_row(text), Choice::Save(name))]
    }

    fn empty_text(&self) -> String {
        match self.kind {
            Kind::Presets => tr("rename-presets-empty"),
            Kind::SavePreset => tr("rename-presets-name"),
            _ => tr("rename-token-menu-empty"),
        }
    }

    fn deletable(&self) -> Option<String> {
        let offset = usize::from(self.presets.last_used.is_some());
        let index = self.selected.checked_sub(offset)?;
        (self.kind == Kind::Presets)
            .then(|| {
                self.presets
                    .named
                    .get(index)
                    .map(|preset| preset.name.clone())
            })
            .flatten()
    }
}

impl FerriteWindow {
    pub(super) fn open_rename_menu(
        &mut self,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_rename_menu_with((kind, RenamePresetList::default()), window, cx);
    }

    pub(super) fn open_rename_menu_with(
        &mut self,
        (kind, presets): (Kind, RenamePresetList),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.multi_rename.open.is_none() {
            return;
        }
        self.remember_rename_field(window, cx);
        let has_filter = matches!(kind, Kind::Tokens(_) | Kind::SavePreset);
        let filter = has_filter.then(|| {
            let placeholder = if kind == Kind::SavePreset {
                tr("rename-presets-name")
            } else {
                tr("rename-token-menu-filter")
            };
            text_field_state(placeholder, window, cx)
        });
        let filter_changed = filter.as_ref().map(|filter| {
            cx.subscribe_in(filter, window, |view, _, event: &InputEvent, _, cx| {
                view.rename_filter_event(event, cx);
            })
        });
        let focus = match &filter {
            Some(filter) => filter.read(cx).focus_handle(cx),
            None => cx.focus_handle(),
        };
        window.focus(&focus, cx);
        if let Some(open) = self.multi_rename.open.as_mut() {
            open.menu = Some(Menu {
                kind,
                selected: 0,
                filter,
                focus,
                presets,
                _filter_changed: filter_changed,
            });
        }
        cx.notify();
    }

    pub(super) fn close_rename_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return false;
        };
        if open.menu.take().is_none() {
            return false;
        }
        window.focus(&open.last_field, cx);
        cx.notify();
        true
    }

    fn rename_filter_event(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if matches!(event, InputEvent::Change) {
            self.move_rename_menu_to(0, cx);
        }
    }

    fn move_rename_menu_to(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(menu) = self
            .multi_rename
            .open
            .as_mut()
            .and_then(|open| open.menu.as_mut())
        {
            menu.selected = index;
            cx.notify();
        }
    }

    fn step_rename_menu(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let Some(menu) = open.menu.as_ref() else {
            return;
        };
        let count = menu
            .choices(&open.form.rules, &open.source.registry, cx)
            .len();
        if count == 0 {
            return;
        }
        let next = if forward {
            (menu.selected + 1) % count
        } else {
            (menu.selected + count - 1) % count
        };
        self.move_rename_menu_to(next, cx);
    }

    fn pick_rename_menu(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let Some(menu) = open.menu.as_ref() else {
            return;
        };
        let choices = menu.choices(&open.form.rules, &open.source.registry, cx);
        let Some((_, choice)) = choices.into_iter().nth(index) else {
            return;
        };
        self.close_rename_menu(window, cx);
        self.apply_rename_choice(choice, window, cx);
    }

    fn apply_rename_choice(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        match choice {
            Choice::Insert(mask, text) => {
                let input = match mask {
                    Mask::Name => open.inputs.name_mask.clone(),
                    Mask::Extension => open.inputs.ext_mask.clone(),
                };
                input.update(cx, |input, cx| input.insert(text, window, cx));
                window.focus(&input.read(cx).focus_handle(cx), cx);
            }
            Choice::NameCase(case) => open.form.rules.name_case = case,
            Choice::ExtCase(case) => open.form.rules.ext_case = case,
            Choice::Load(rules) => return self.load_preset(&rules, window, cx),
            Choice::Save(name) => return self.save_preset(name, cx),
            Choice::Delete(name) => return self.delete_preset(name, window, cx),
        }
        self.rules_changed(window, cx);
    }

    fn delete_rename_menu_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let Some(menu) = open.menu.as_ref() else {
            return;
        };
        if let Some(name) = menu.deletable() {
            let presets = menu.presets.clone();
            self.open_rename_menu_with((Kind::DeletePreset(name), presets), window, cx);
        }
    }

    fn rename_menu_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.multi_rename.open.as_ref()?;
        let menu = open.menu.as_ref()?;
        let rows: Vec<MenuRow> = menu
            .choices(&open.form.rules, &open.source.registry, cx)
            .into_iter()
            .map(|(row, _)| row)
            .collect();
        let weak = cx.weak_entity();
        let mut list = FieldMenu::new("rename-menu", rows)
            .selected(menu.selected)
            .empty(menu.empty_text())
            .on_pick(move |index, _, window, cx| {
                let _ = weak.update(cx, |view, cx| view.pick_rename_menu(index, window, cx));
            });
        if let Some(filter) = &menu.filter {
            list = list.filter(filter);
        }
        let context = if menu.kind == Kind::Presets {
            super::keys::contexts(&["ContextMenu", super::keys::PREVIEW_CONTEXT])
        } else {
            super::keys::contexts(&["ContextMenu"])
        };
        Some(
            div()
                .id("rename-menu-keys")
                .key_context(context)
                .track_focus(&menu.focus)
                .on_action(cx.listener(|view, _: &MenuUp, _, cx| view.step_rename_menu(false, cx)))
                .on_action(cx.listener(|view, _: &MenuDown, _, cx| view.step_rename_menu(true, cx)))
                .on_action(cx.listener(|view, _: &MenuEnter, window, cx| {
                    let selected = view.rename_menu_selected();
                    view.pick_rename_menu(selected, window, cx);
                }))
                .on_action(cx.listener(|view, _: &MenuEscape, window, cx| {
                    view.close_rename_menu(window, cx);
                }))
                .on_action(cx.listener(|view, _: &ToggleRow, window, cx| {
                    view.delete_rename_menu_row(window, cx);
                }))
                .child(list)
                .into_any_element(),
        )
    }

    fn rename_menu_selected(&self) -> usize {
        self.multi_rename
            .open
            .as_ref()
            .and_then(|open| open.menu.as_ref())
            .map_or(0, |menu| menu.selected)
    }

    fn rename_menu_kind(&self) -> Option<&Kind> {
        self.multi_rename
            .open
            .as_ref()?
            .menu
            .as_ref()
            .map(|menu| &menu.kind)
    }

    pub(super) fn with_field_menu(&self, fields: Fields, cx: &mut Context<Self>) -> Fields {
        let slot = match self.rename_menu_kind() {
            Some(Kind::Tokens(mask)) => MenuSlot::Mask(*mask),
            Some(Kind::NameCase) => MenuSlot::NameCase,
            Some(Kind::ExtCase) => MenuSlot::ExtensionCase,
            _ => return fields,
        };
        match self.rename_menu_element(cx) {
            Some(menu) => fields.menu(slot, menu),
            None => fields,
        }
    }

    pub(super) fn preset_menu_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        match self.rename_menu_kind()? {
            Kind::Presets | Kind::SavePreset | Kind::DeletePreset(_) => {
                self.rename_menu_element(cx)
            }
            _ => None,
        }
    }
}
