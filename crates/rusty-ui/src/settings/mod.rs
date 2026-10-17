mod controls;
mod fields;
mod steps;
mod values;
mod view;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, RenderOnce, Role, ScrollHandle};

pub use fields::{
    Choice, Control, DENSITIES, FIELDS, Field, FieldSpec, HIDDEN, Look, SORTS, Section, THEMES,
};
pub use steps::{Nudge, first_row, moved, next_section, nudged, rows_in, section_of};
pub use values::{Change, Value, Values};

use crate::prelude::*;
use crate::side_sheet::{SheetFrame, SheetState, SideSheet};
use crate::tokens;

pub const CONTEXT: &str = "SettingsSheet";
pub const FIELD_CONTEXT: &str = "SettingsField";

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
        UndoChange,
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
    Close,
}

type Handler = Rc<dyn Fn(&SheetEvent, &mut Window, &mut App)>;

pub fn sheet(state: &Entity<SheetState>, frame: SheetFrame, body: SettingsBody) -> SideSheet {
    let title = div().font_weight(FontWeight::SEMIBOLD).child("Settings");
    let sheet = SideSheet::new("settings-sheet", state, frame)
        .label("Settings")
        .header(title);
    if body.shortcuts.is_some() {
        return sheet
            .body(body)
            .hint(&["↑", "↓"], "Move")
            .hint(&["enter"], "Record")
            .hint(&["ctrl-r"], "Reset")
            .hint(&["tab"], "Section");
    }
    sheet
        .body(body)
        .hint(&["↑", "↓"], "Move")
        .hint(&["←", "→"], "Change")
        .hint(&["tab"], "Section")
        .hint(&["f3"], "TOML line")
        .hint(&["ctrl-z"], "Undo")
}

#[derive(IntoElement)]
pub struct SettingsBody {
    values: Values,
    row: usize,
    error: Option<SharedString>,
    editor: Option<AnyElement>,
    shortcuts: Option<AnyElement>,
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
            Section::Editor
        } else {
            Section::Appearance
        };
        match (self.shortcuts.is_some(), forward) {
            (true, true) => SheetEvent::Select(first_row(Section::Appearance)),
            (true, false) => SheetEvent::Select(first_row(Section::Editor)),
            (false, _) if section_of(self.row) == edge => SheetEvent::Shortcuts,
            (false, _) => SheetEvent::Select(next_section(self.row, forward)),
        }
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
    let up = on_event(body, Some(SheetEvent::Select(moved(row, false))));
    let down = on_event(body, Some(SheetEvent::Select(moved(row, true))));
    let back = on_event(body, body.event_for(Nudge::Back));
    let forward = on_event(body, body.event_for(Nudge::Forward));
    let flip = on_event(body, body.flip_event());
    let edit = on_event(body, body.edit_event());
    let locate = on_event(body, field.map(SheetEvent::Locate));
    let undo = on_event(body, Some(SheetEvent::Undo));
    root.on_action(move |_: &RowUp, window, cx| up(window, cx))
        .on_action(move |_: &RowDown, window, cx| down(window, cx))
        .on_action(move |_: &ValueBack, window, cx| back(window, cx))
        .on_action(move |_: &ValueForward, window, cx| forward(window, cx))
        .on_action(move |_: &ValueFlip, window, cx| flip(window, cx))
        .on_action(move |_: &EditValue, window, cx| edit(window, cx))
        .on_action(move |_: &OpenKey, window, cx| locate(window, cx))
        .on_action(move |_: &UndoChange, window, cx| undo(window, cx))
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
            send: self.on_event.clone(),
        };
        let shown = self.shortcuts.is_none().then_some(section);
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
                    .child(content),
            );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(keyed)
            .children(self.editor)
    }
}
