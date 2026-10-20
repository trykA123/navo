mod controls;
mod fields;
mod find;
mod find_bar;
mod row_state;
mod steps;
mod upgrade;
mod values;
mod view;
mod viewer_fields;
mod window;
mod window_fields;

#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, RenderOnce, Role, ScrollHandle};

pub use fields::{
    Choice, Control, DENSITIES, FIELDS, Field, FieldSpec, HIDDEN, HINTS, Look, PRESETS, SORTS,
    Section, THEMES,
};
pub use find::{Filter, Line, lines};
pub use find_bar::find_field;
pub use steps::{Nudge, child_of, first_row, moved, next_section, nudged, rows_in, section_of};
pub use upgrade::{DefaultShift, Moved};
pub use values::{Change, Value, Values};
pub use window::WindowParts;

use crate::prelude::*;
use crate::side_sheet::{SheetFrame, SheetState, SideSheet};
use crate::tokens;

pub const CONTEXT: &str = "SettingsSheet";
const LAST_SECTION: Section = Section::ALL[Section::ALL.len() - 1];
pub const FIELD_CONTEXT: &str = "SettingsField";
pub const FIND_CONTEXT: &str = "SettingsFind";

gpui_kit::actions!(
    settings_sheet,
    [
        ToggleSettings,
        RowUp,
        RowDown,
        ValueBack,
        ValueForward,
        ValueFlip,
        NextSection,
        PreviousSection,
        EditValue,
        OpenKey,
        ResetValue,
        UndoChange,
        FindSetting,
        ClearFind,
        FindToList,
        CloseSettings,
        CancelEdit
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "settings_sheet");
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SheetEvent {
    Select(usize),
    Shortcuts,
    Set(Change),
    Edit(Field),
    Locate(Field),
    Undo,
    Find,
    Keep(Field),
    DismissUpgrade,
    Close,
}

type Handler = Rc<dyn Fn(&SheetEvent, &mut Window, &mut App)>;

pub fn sheet(state: &Entity<SheetState>, frame: SheetFrame, body: SettingsBody) -> SideSheet {
    let sheet = SideSheet::new("settings-sheet", state, frame)
        .label("Settings")
        .header(SheetTitle);
    if body.shortcuts.is_some() {
        return sheet
            .body(body)
            .hint(&["↑", "↓"], "Move")
            .hint(&["enter"], "Record")
            .hint(&["ctrl-r"], "Reset")
            .hint(&["tab"], "Section");
    }
    let can_reset = body.reset_event().is_some();
    let sheet = sheet
        .body(body)
        .hint(&["↑", "↓"], "Move")
        .hint(&["←", "→"], "Change")
        .hint(&["tab"], "Section")
        .hint(&["f3"], "TOML line");
    let sheet = if can_reset {
        sheet.hint(&["f8"], "Reset")
    } else {
        sheet
    };
    sheet.hint(&["ctrl-z"], "Undo").hint(&["ctrl-f"], "Find")
}

#[derive(IntoElement)]
struct SheetTitle;

impl RenderOnce for SheetTitle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Settings"))
            .child(row_state::live_chip(t))
    }
}

#[derive(IntoElement)]
pub struct SettingsBody {
    values: Values,
    row: usize,
    error: Option<SharedString>,
    editor: Option<AnyElement>,
    shortcuts: Option<AnyElement>,
    filter: Filter,
    find: Option<AnyElement>,
    upgrade: Vec<Moved>,
    focus: Option<FocusHandle>,
    scroll: Option<ScrollHandle>,
    on_event: Option<Handler>,
}

impl SettingsBody {
    pub fn new(values: Values, row: usize) -> Self {
        Self {
            values,
            row: row.min(FIELDS.len() - 1),
            error: None,
            editor: None,
            shortcuts: None,
            filter: Filter::default(),
            find: None,
            upgrade: Vec::new(),
            focus: None,
            scroll: None,
            on_event: None,
        }
    }

    #[must_use]
    pub fn error(mut self, message: Option<SharedString>) -> Self {
        self.error = message;
        self
    }

    #[must_use]
    pub fn editor(mut self, editor: Option<AnyElement>) -> Self {
        self.editor = editor;
        self
    }

    #[must_use]
    pub fn shortcuts(mut self, page: Option<AnyElement>) -> Self {
        self.shortcuts = page;
        self
    }

    #[must_use]
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    #[must_use]
    pub fn find(mut self, bar: Option<AnyElement>) -> Self {
        self.find = bar;
        self
    }

    #[must_use]
    pub fn upgrade(mut self, items: Vec<Moved>) -> Self {
        self.upgrade = items;
        self
    }

    #[must_use]
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    #[must_use]
    pub fn track_scroll(mut self, scroll: &ScrollHandle) -> Self {
        self.scroll = Some(scroll.clone());
        self
    }

    #[must_use]
    pub fn on_event(
        mut self,
        handler: impl Fn(&SheetEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }

    fn event_for(&self, nudge: Nudge) -> Option<SheetEvent> {
        let spec = FIELDS.get(self.row)?;
        let value = nudged(spec, self.values.get(spec.field), nudge)?;
        Some(SheetEvent::Set(Change {
            field: spec.field,
            value,
        }))
    }

    fn flip_event(&self) -> Option<SheetEvent> {
        let spec = FIELDS.get(self.row)?;
        match spec.control {
            Control::Text => Some(SheetEvent::Edit(spec.field)),
            _ => self.event_for(Nudge::Flip),
        }
    }

    fn section_event(&self, forward: bool) -> SheetEvent {
        let edge = if forward {
            LAST_SECTION
        } else {
            Section::Appearance
        };
        match (self.shortcuts.is_some(), forward) {
            (true, true) => SheetEvent::Select(first_row(Section::Appearance)),
            (true, false) => SheetEvent::Select(first_row(LAST_SECTION)),
            (false, _) if self.filter.is_empty() && section_of(self.row) == edge => {
                SheetEvent::Shortcuts
            }
            (false, _) => SheetEvent::Select(next_section(&self.filter, self.row, forward)),
        }
    }

    fn reset_event(&self) -> Option<SheetEvent> {
        let spec = FIELDS.get(self.row)?;
        let is_shown = self.shortcuts.is_none() && self.values.is_changed(spec.field);
        is_shown.then(|| SheetEvent::Set(Change::reset(spec.field)))
    }

    fn edit_event(&self) -> Option<SheetEvent> {
        let spec = FIELDS.get(self.row)?;
        matches!(spec.control, Control::Text | Control::Number)
            .then_some(SheetEvent::Edit(spec.field))
    }
}

fn send(handler: Option<&Handler>, event: Option<SheetEvent>, window: &mut Window, cx: &mut App) {
    if let (Some(handler), Some(event)) = (handler, event) {
        handler(&event, window, cx);
    }
}

fn on_event(
    body: &SettingsBody,
    event: Option<SheetEvent>,
) -> impl Fn(&mut Window, &mut App) + use<> {
    let handler = body.on_event.clone();
    move |window: &mut Window, cx: &mut App| send(handler.as_ref(), event.clone(), window, cx)
}

fn register(root: Div, body: &SettingsBody) -> Div {
    let next = on_event(body, Some(body.section_event(true)));
    let previous = on_event(body, Some(body.section_event(false)));
    let close = on_event(body, Some(SheetEvent::Close));
    let root = root
        .on_action(move |_: &NextSection, window, cx| next(window, cx))
        .on_action(move |_: &PreviousSection, window, cx| previous(window, cx))
        .on_action(move |_: &CloseSettings, window, cx| close(window, cx));
    if body.shortcuts.is_some() {
        return root;
    }
    register_fields(root, body)
}

fn register_fields(root: Div, body: &SettingsBody) -> Div {
    let row = body.row;
    let field = FIELDS.get(row).map(|spec| spec.field);
    let filter = &body.filter;
    let up = on_event(body, Some(SheetEvent::Select(moved(filter, row, false))));
    let down = on_event(body, Some(SheetEvent::Select(moved(filter, row, true))));
    let find = on_event(body, Some(SheetEvent::Find));
    let back = on_event(body, body.event_for(Nudge::Back));
    let forward = on_event(body, body.event_for(Nudge::Forward));
    let flip = on_event(body, body.flip_event());
    let edit = on_event(body, body.edit_event());
    let locate = on_event(body, field.map(SheetEvent::Locate));
    let reset = on_event(body, body.reset_event());
    let undo = on_event(body, Some(SheetEvent::Undo));
    root.on_action(move |_: &RowUp, window, cx| up(window, cx))
        .on_action(move |_: &RowDown, window, cx| down(window, cx))
        .on_action(move |_: &ValueBack, window, cx| back(window, cx))
        .on_action(move |_: &ValueForward, window, cx| forward(window, cx))
        .on_action(move |_: &ValueFlip, window, cx| flip(window, cx))
        .on_action(move |_: &EditValue, window, cx| edit(window, cx))
        .on_action(move |_: &OpenKey, window, cx| locate(window, cx))
        .on_action(move |_: &ResetValue, window, cx| reset(window, cx))
        .on_action(move |_: &UndoChange, window, cx| undo(window, cx))
        .on_action(move |_: &FindSetting, window, cx| find(window, cx))
}

impl RenderOnce for SettingsBody {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let keys = register(div().key_context(CONTEXT), &self);
        let section = section_of(self.row);
        let look = view::Look {
            theme: &t,
            values: &self.values,
            row: self.row,
            filter: &self.filter,
            send: self.on_event.clone(),
        };
        let shown = self.shortcuts.is_none().then_some(section);
        let notice = shown
            .filter(|_| !self.upgrade.is_empty())
            .map(|_| upgrade::card(&t, &self.upgrade, self.on_event.as_ref()));
        let footer = shown.map(|_| row_state::footer(&t, &self.values));
        let content = match self.shortcuts {
            Some(page) => page,
            None => view::content(&look, section, self.scroll.as_ref()).into_any_element(),
        };
        let keyed = keys
            .id("settings-body")
            .role(Role::Group)
            .aria_label("Settings")
            .test_support()
            .when_some(self.focus.as_ref(), |root, focus| root.track_focus(focus))
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(view::nav(&look, shown))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .children(self.error.map(|message| view::error_line(&t, message)))
                    .children(notice)
                    .child(content)
                    .children(footer),
            );
        framed(self.find.filter(|_| shown.is_some()), keyed, self.editor)
    }
}

fn framed(find: Option<AnyElement>, body: impl IntoElement, editor: Option<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .children(find)
        .child(body)
        .children(editor)
}
