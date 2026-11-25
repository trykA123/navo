//! Settings sheet data, events, field navigation and body rendering.

mod body;
mod clipboard_fields;
mod clipboard_values;
mod compare_fields;
mod compare_options;
mod controls;
mod diagnostics_page;
mod dropdown;
mod field_visibility;
mod fields;
mod find;
mod find_bar;
mod fold;
mod general_fields;
mod git_fields;
mod git_page;
mod git_values;
mod help_page;
mod index_page;
mod index_progress;
mod inspector;
mod keyboard_page;
mod nav;
mod nested;
mod page_keys;
mod pages;
mod row_state;
mod search_fields;
mod search_page;
mod steps;
mod stored;
mod summon_key;
mod tray_fields;
mod update_fields;
mod upgrade;
mod values;
mod view;
mod viewer_fields;
mod window;
mod window_fields;

#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod hidden_tests;
#[cfg(test)]
mod pages_tests;
#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Entity, FocusHandle, RenderOnce, Role, ScrollHandle, Toggled};

pub use compare_options::compare_options;
pub use fields::{
    COLOR_THEMES, Choice, Control, DENSITIES, FIELDS, Field, FieldSpec, HIDDEN, HINTS, Look,
    PRESETS, SORTS, SYNTAX_THEMES, Section, THEMES,
};
pub use find::{Filter, Line, lines};
pub use find_bar::find_field;
pub use fold::{Folds, clamped_row, fold_row, outer_row};
pub use git_page::GitPage;
pub use index_progress::{IndexProgress, IndexState, Split, VolumeProgress};
pub use search_page::{SearchPage, ServiceStatus};
pub use steps::{
    DIAGNOSTICS_ROW, HELP_ROW, Nudge, child_of, digit_of, first_row, moved, next_section, nudged,
    rows_in, section_of,
};
pub use stored::stored_value;
pub use summon_key::{SummonKey, SummonKeyError, parse_summon_key};
pub use upgrade::{DefaultShift, Moved};
pub use values::{Change, Value, Values};
pub use window::WindowParts;

use crate::prelude::*;
use crate::primitives::popup_menu::{self, PopupMenu};
use crate::side_sheet::{SheetFrame, SheetState, SideSheet};
use crate::tokens;
use crate::window_view::UpdateView;

pub use diagnostics_page::DiagnosticRow;

pub const CONTEXT: &str = "SettingsSheet";
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
        CancelEdit,
        SaveDiagnosticReport
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "settings_sheet");
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SheetEvent {
    Select(usize),
    /// Opens (`true`) or closes the fold on this fold row, and selects the row.
    Fold(usize, bool),
    Shortcuts,
    /// Opens the Shortcuts page focused on the named command from a Find hit.
    Shortcut(&'static str),
    Set(Change),
    Edit(Field),
    Locate(Field),
    Undo,
    Find,
    /// Toggles filtering every page down to settings that differ from their default.
    ToggleChangedFilter,
    Keep(Field),
    DismissUpgrade,
    RefreshDiagnostics,
    ToggleDiagnosticsFold,
    SelectDiagnostic(usize),
    OpenDataFolder,
    OpenDiagnosticPath(SharedString),
    CopyDiagnosticPath(SharedString),
    OpenGuide,
    CheckUpdates,
    RestartUpdate,
    OpenReleases,
    ClearHistory,
    InstallService,
    RemoveService,
    Close,
}

type Handler = Rc<dyn Fn(&SheetEvent, &mut Window, &mut App)>;

/// Builds the Settings dialog: a floating sheet with the title, `body` and the key hints
/// that fit the shown page (Shortcuts or a settings section).
pub fn sheet(state: &Entity<SheetState>, frame: SheetFrame, body: SettingsBody) -> SideSheet {
    let title = SheetTitle {
        changed_count: body.values.changed_count(),
        is_changed_only: body.filter.is_changed_only(),
        on_event: body.on_event.clone(),
    };
    let sheet = SideSheet::new("settings-sheet", state, frame)
        .floating()
        .label("Settings")
        .header(title);
    if body.shortcuts.is_some() {
        return sheet
            .body(body)
            .hint(&["↑", "↓"], "Move")
            .hint(&["enter"], "Record")
            .hint(&["ctrl-r"], "Reset")
            .hint(&["ctrl-tab"], "Page");
    }
    let can_reset = body.reset_event().is_some();
    let sheet = sheet
        .body(body)
        .hint(&["↑", "↓"], "Move")
        .hint(&["←", "→"], "Change")
        .hint(&["ctrl-tab"], "Page")
        .hint(&["f3"], "TOML line");
    let sheet = if can_reset {
        sheet.hint(&["f8"], "Reset")
    } else {
        sheet
    };
    sheet.hint(&["ctrl-z"], "Undo").hint(&["ctrl-f"], "Find")
}

#[derive(IntoElement)]
struct SheetTitle {
    changed_count: usize,
    is_changed_only: bool,
    on_event: Option<Handler>,
}

fn changed_chip(
    t: &tokens::Theme,
    count: usize,
    is_active: bool,
    handler: Option<&Handler>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut args = navo_l10n::FluentArgs::new();
    args.set("count", count as i64);
    let label: SharedString = navo_l10n::tr_args("settings-changed-chip", &args).into();
    let focus = window
        .use_keyed_state("settings-changed-chip-focus", cx, |_, cx| {
            cx.focus_handle().tab_stop(true).tab_index(1)
        })
        .read(cx)
        .clone();
    let theme = t.selected_content(is_active);
    let (bg, edge, text_color) = if is_active {
        (
            theme.colors.selection,
            theme.colors.accent,
            theme.colors.accent_text,
        )
    } else {
        (t.surfaces.field, t.surfaces.card_edge, t.colors.text)
    };
    let click_send = handler.cloned();
    div()
        .id("settings-changed-chip")
        .role(Role::Button)
        .aria_label(label.clone())
        .aria_selected(is_active)
        .aria_toggled(if is_active {
            Toggled::True
        } else {
            Toggled::False
        })
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.xxs)
        .rounded(t.radii.pill)
        .border_1()
        .border_color(edge)
        .bg(bg)
        .text_size(t.typography.small)
        .text_color(text_color)
        .when(is_active, |chip| chip.font_weight(FontWeight::MEDIUM))
        .cursor_pointer()
        .track_focus(&focus)
        .focus_visible(|chip| chip.border_color(t.colors.accent))
        .when(!is_active, |chip| {
            chip.hover(|chip| chip.bg(t.colors.hover))
        })
        .on_click(move |_, window, cx| {
            send(
                click_send.as_ref(),
                Some(SheetEvent::ToggleChangedFilter),
                window,
                cx,
            );
        })
        .child(row_state::dot(&theme))
        .child(label)
        .into_any_element()
}

impl RenderOnce for SheetTitle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let changed = (self.changed_count > 0).then(|| {
            changed_chip(
                &t,
                self.changed_count,
                self.is_changed_only,
                self.on_event.as_ref(),
                window,
                cx,
            )
        });
        let close_send = self.on_event.clone();
        let find_send = self.on_event.clone();
        let undo_send = self.on_event.clone();
        div()
            .key_context(CONTEXT)
            .on_action(move |_: &CloseSettings, window, cx| {
                send(close_send.as_ref(), Some(SheetEvent::Close), window, cx);
            })
            .on_action(move |_: &FindSetting, window, cx| {
                send(find_send.as_ref(), Some(SheetEvent::Find), window, cx);
            })
            .on_action(move |_: &UndoChange, window, cx| {
                send(undo_send.as_ref(), Some(SheetEvent::Undo), window, cx);
            })
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .child(
                div()
                    .font_family(t.typography.brand_font.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(row_state::live_chip(&t))
            .children(changed)
    }
}

#[derive(IntoElement)]
pub struct SettingsBody {
    values: Values,
    row: usize,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    editor: Option<AnyElement>,
    shortcuts: Option<AnyElement>,
    filter: Filter,
    find: Option<AnyElement>,
    upgrade: Vec<Moved>,
    page: SearchPage,
    git: GitPage,
    index: IndexProgress,
    diagnostics: Vec<DiagnosticRow>,
    diagnostics_open: bool,
    selected_diagnostic: Option<usize>,
    version: SharedString,
    update: UpdateView,
    restart_offered: bool,
    focus: Option<FocusHandle>,
    scroll: Option<ScrollHandle>,
    on_event: Option<Handler>,
    summon_key_taken: bool,
    clipboard_key_taken: bool,
}

impl SettingsBody {
    fn shown_spec(&self) -> Option<&'static FieldSpec> {
        if self.row == HELP_ROW && self.shortcuts.is_none() {
            return Some(Field::CheckUpdates.spec());
        }
        FIELDS.get(self.row).filter(|spec| self.filter.admits(spec))
    }

    fn event_for(&self, nudge: Nudge) -> Option<SheetEvent> {
        let spec = self.shown_spec()?;
        if !self.is_enabled(spec.field) {
            return None;
        }
        let value = match spec.control {
            Control::Tool => self.git.nudged(self.values.git_tool(), nudge)?,
            _ => nudged(spec, self.values.get(spec.field), nudge)?,
        };
        Some(SheetEvent::Set(Change {
            field: spec.field,
            value,
        }))
    }

    fn is_enabled(&self, field: Field) -> bool {
        view::is_row_enabled(&self.values, &self.update, field)
    }

    fn flip_event(&self) -> Option<SheetEvent> {
        let spec = self.shown_spec()?;
        match spec.control {
            Control::Text => Some(SheetEvent::Edit(spec.field)),
            _ => self.event_for(Nudge::Flip),
        }
    }

    /// The next or previous page; the shortcut list counts as the Keyboard page.
    fn section_event(&self, forward: bool) -> Option<SheetEvent> {
        let row = match self.shortcuts {
            Some(_) => first_row(Section::Keyboard),
            None => self.row,
        };
        let next = next_section(&self.filter, row, forward);
        let is_shown = FIELDS.get(next).is_none_or(|spec| self.filter.admits(spec));
        is_shown.then_some(SheetEvent::Select(next))
    }

    fn reset_event(&self) -> Option<SheetEvent> {
        let spec = self.shown_spec()?;
        let is_shown = self.shortcuts.is_none() && self.values.is_changed(spec.field);
        is_shown.then(|| SheetEvent::Set(Change::reset(spec.field)))
    }

    fn edit_event(&self) -> Option<SheetEvent> {
        let spec = self.shown_spec()?;
        let is_editable =
            matches!(spec.control, Control::Text | Control::Number) && self.is_enabled(spec.field);
        is_editable.then_some(SheetEvent::Edit(spec.field))
    }
    fn focused_dropdown(
        &self,
        menus: &[(Field, Entity<PopupMenu>)],
    ) -> Option<(Entity<PopupMenu>, usize, usize)> {
        let spec = self.shown_spec()?;
        if !self.is_enabled(spec.field) {
            return None;
        }
        let (Control::Choices(options, Look::Segments), Value::Pick(at)) =
            (spec.control, self.values.get(spec.field))
        else {
            return None;
        };
        let menu = menus
            .iter()
            .find(|(field, _)| *field == spec.field)?
            .1
            .clone();
        Some((menu, *at, options.len()))
    }

    fn diagnostic_step_event(&self, down: bool) -> Option<SheetEvent> {
        if !self.diagnostics_open || self.diagnostics.is_empty() {
            return None;
        }
        if let Some(index) = self.selected_diagnostic {
            if down {
                let next = (index + 1).min(self.diagnostics.len() - 1);
                return Some(SheetEvent::SelectDiagnostic(next));
            }
            if index > 0 {
                return Some(SheetEvent::SelectDiagnostic(index - 1));
            }
            return Some(SheetEvent::Select(HELP_ROW));
        }
        (down && self.row == HELP_ROW).then_some(SheetEvent::SelectDiagnostic(0))
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

fn dismiss_menus(menus: &[Entity<PopupMenu>], cx: &mut App) {
    for menu in menus {
        let _ = popup_menu::dismiss(menu, cx);
    }
}

pub(super) fn on_event_dismissing(
    body: &SettingsBody,
    event: Option<SheetEvent>,
    menus: &[(Field, Entity<PopupMenu>)],
) -> impl Fn(&mut Window, &mut App) + use<> {
    let handler = body.on_event.clone();
    let open_menus: Vec<Entity<PopupMenu>> = menus.iter().map(|(_, menu)| menu.clone()).collect();
    move |window: &mut Window, cx: &mut App| {
        dismiss_menus(&open_menus, cx);
        send(handler.as_ref(), event.clone(), window, cx);
    }
}

fn register(root: Div, body: &SettingsBody, menus: &[(Field, Entity<PopupMenu>)]) -> Div {
    let next = on_event_dismissing(body, body.section_event(true), menus);
    let previous = on_event_dismissing(body, body.section_event(false), menus);
    let close = on_event_dismissing(body, Some(SheetEvent::Close), menus);
    let root = root
        .on_action(move |_: &NextSection, window, cx| next(window, cx))
        .on_action(move |_: &PreviousSection, window, cx| previous(window, cx))
        .on_action(move |_: &CloseSettings, window, cx| close(window, cx));
    let root = page_keys::register(root, body, menus);
    if body.shortcuts.is_some() {
        return root;
    }
    register_fields(root, body, menus)
}

fn register_fields(root: Div, body: &SettingsBody, menus: &[(Field, Entity<PopupMenu>)]) -> Div {
    let row = body.row;
    let filter = &body.filter;
    let up_event = body
        .diagnostic_step_event(false)
        .unwrap_or_else(|| SheetEvent::Select(moved(filter, row, false)));
    let down_event = body
        .diagnostic_step_event(true)
        .unwrap_or_else(|| keyboard_page::down_event(filter, row));
    let up = on_event(body, Some(up_event));
    let down = on_event(body, Some(down_event));
    let find = on_event_dismissing(body, Some(SheetEvent::Find), menus);
    let back = fold::left_event(filter, row).or_else(|| body.event_for(Nudge::Back));
    let forward = fold::right_event(filter, row).or_else(|| body.event_for(Nudge::Forward));
    let (back, forward) = (on_event(body, back), on_event(body, forward));
    let flip = on_event(body, body.flip_event());
    let edit = on_event(body, body.edit_event());
    let dropdown_for_flip = body.focused_dropdown(menus);
    let dropdown_for_edit = dropdown_for_flip.clone();
    let locate = on_event(
        body,
        body.shown_spec().map(|spec| SheetEvent::Locate(spec.field)),
    );
    let reset = on_event(body, body.reset_event());
    let undo = on_event(body, Some(SheetEvent::Undo));
    root.on_action(move |_: &RowUp, window, cx| up(window, cx))
        .on_action(move |_: &RowDown, window, cx| down(window, cx))
        .on_action(move |_: &ValueBack, window, cx| back(window, cx))
        .on_action(move |_: &ValueForward, window, cx| forward(window, cx))
        .on_action(move |_: &ValueFlip, window, cx| {
            if let Some((menu, at, count)) = &dropdown_for_flip {
                dropdown::open_at(menu, (*at, *count), window, cx);
            } else {
                flip(window, cx);
            }
        })
        .on_action(move |_: &EditValue, window, cx| {
            if let Some((menu, at, count)) = &dropdown_for_edit {
                dropdown::open_at(menu, (*at, *count), window, cx);
            } else {
                edit(window, cx);
            }
        })
        .on_action(move |_: &OpenKey, window, cx| locate(window, cx))
        .on_action(move |_: &ResetValue, window, cx| reset(window, cx))
        .on_action(move |_: &UndoChange, window, cx| undo(window, cx))
        .on_action(move |_: &FindSetting, window, cx| find(window, cx))
}

impl RenderOnce for SettingsBody {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.filter = self.filter.marking(&self.values);
        if self.values.changed_count() == 0 {
            self.filter = self.filter.changed_only(false);
        }
        let section = section_of(self.row);
        let menus =
            dropdown::visible_menus(&self.filter, section, self.shortcuts.is_some(), window, cx);
        let keys = register(div().key_context(CONTEXT), &self, &menus);
        let look = view::Look {
            theme: &t,
            values: &self.values,
            row: keyboard_page::shown_row(self.row, self.shortcuts.is_some()),
            filter: &self.filter,
            page: &self.page,
            git: &self.git,
            index: &self.index,
            keymap: crate::commands::active(cx),
            diagnostics: &self.diagnostics,
            diagnostics_open: self.diagnostics_open,
            selected_diagnostic: self.selected_diagnostic,
            version: &self.version,
            update: &self.update,
            restart_offered: self.restart_offered,
            send: self.on_event.clone(),
            summon_key_taken: self.summon_key_taken,
            clipboard_key_taken: self.clipboard_key_taken,
            menus: &menus,
        };
        let shown = self.shortcuts.is_none().then_some(section);
        let notice = shown
            .filter(|_| !self.upgrade.is_empty())
            .map(|_| upgrade::card(&t, &self.upgrade, self.on_event.as_ref()));
        let footer = shown.map(|_| row_state::footer(&t, &self.values));
        let content =
            keyboard_page::content(&look, section, self.shortcuts, self.scroll.as_ref(), cx);
        let nav = nav::NavState::new(shown, window);
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
            .tab_group()
            .tab_index(2)
            .child(nav::nav(&look, &nav))
            .child(
                column()
                    .children(self.error.map(|message| view::error_line(&t, message)))
                    .children(self.notice.map(|message| view::notice_line(&t, message)))
                    .children(notice)
                    .child(content)
                    .children(footer),
            );
        framed(self.find.filter(|_| shown.is_some()), keyed, self.editor)
    }
}

fn column() -> Div {
    div().flex().flex_col().flex_1().min_h_0().min_w_0()
}

fn framed(find: Option<AnyElement>, body: impl IntoElement, editor: Option<AnyElement>) -> Div {
    let find = find.map(|bar| div().flex_none().tab_group().tab_index(0).child(bar));
    column().children(find).child(body).children(editor)
}
