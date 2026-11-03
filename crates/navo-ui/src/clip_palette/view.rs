use std::fmt;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement,
    Styled, Subscription, Window, div, px,
};
use navo_core::Location;
use navo_l10n::tr;

use super::actions::Overlay;
use super::edit_form::Form;
use super::keys::{CONTEXT, on_keys};
use super::list::Paint;
use super::manage::Manage;
use super::note_editor::{FolderProbe, NoteEditor};
use super::preview::{self, Look};
use super::refresh::system_now;
use super::repoint::Repoint;
use super::rows::RowElement;
use super::state::PaletteState;
use super::{ClipMode, ClipboardHooks};
use crate::primitives::{ListScroll, TextFieldState, text_field_state};
use crate::tokens::Theme;

const ROOT: &str = "clip-palette";
const WIDTH: f32 = 720.;
const HEIGHT: f32 = 400.;
pub(super) const LIST_WIDTH: f32 = 264.;
const DOCK_BELOW: f32 = 560.;
const DOCK_HEIGHT: f32 = 128.;
const NOTE_DOCK_HEIGHT: f32 = 168.;
const MARGIN: f32 = 8.;

/// Where the palette runs: in Navo's window, or in the pop-up over another app, where Enter
/// pastes into that app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    #[default]
    Window,
    Anywhere,
}

pub struct ClipPaletteProps {
    pub hooks: Arc<dyn ClipboardHooks>,
    pub place: Place,
    pub folder: Option<Location>,
    /// The key that opens the palette, as a key string such as `ctrl-shift-v`.
    pub key_cap: Option<SharedString>,
    /// Fixes the clock the ages are measured from; the system clock when `None`.
    pub now_ms: Option<i64>,
    /// The width the palette fits into; the window width when `None`.
    pub width: Option<Pixels>,
    /// Finds the notes whose folder is gone; without it none is tagged.
    pub folder_probe: Option<FolderProbe>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ClipPaletteEvent {
    Dismissed,
    /// Enter on a path: the host opens this folder in the active pane.
    GoTo(Location),
    /// Ctrl+V on a file list: the list is back on the file clipboard; the host pastes it
    /// into the active pane.
    PasteIntoPane,
    /// Enter from anywhere: the item is on the clipboard; the host hides the pop-up and
    /// pastes into the app it came from.
    PasteBack,
}

/// `Debug` hides the path of `GoTo`, which came from the clipboard.
impl fmt::Debug for ClipPaletteEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dismissed => "Dismissed",
            Self::GoTo(_) => "GoTo(..)",
            Self::PasteIntoPane => "PasteIntoPane",
            Self::PasteBack => "PasteBack",
        })
    }
}

pub struct ClipPalette {
    pub(super) hooks: Arc<dyn ClipboardHooks>,
    pub(super) place: Place,
    pub(super) folder: Option<Location>,
    pub(super) key_cap: Option<SharedString>,
    pub(super) fixed_now: Option<i64>,
    width: Option<Pixels>,
    pub(super) now_ms: i64,
    pub(super) state: PaletteState,
    pub(super) note: NoteEditor,
    pub(super) field: Entity<TextFieldState>,
    pub(super) overlay: Overlay,
    /// The library, while it replaces the palette body.
    pub(super) manage: Option<Manage>,
    /// The new or edit form, while it replaces the palette body.
    pub(super) form: Option<Form>,
    /// A note waiting for a yes to point at the active folder.
    pub(super) repoint: Option<Repoint>,
    /// Whether the first-open line shows; it goes when the palette is reset.
    pub(super) intro: bool,
    /// A hidden pop-up keeps no rows and answers no search until it is shown again.
    pub(super) dormant: bool,
    scroll: ListScroll,
    _field: Subscription,
}

impl EventEmitter<ClipPaletteEvent> for ClipPalette {}

impl Focusable for ClipPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl ClipPalette {
    pub fn new(props: ClipPaletteProps, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = text_field_state(tr("clip-search-placeholder"), window, cx);
        let subscription = cx.subscribe(&field, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refresh(false, cx);
            }
        });
        window.focus(&field.focus_handle(cx), cx);
        let intro = props.hooks.take_intro();
        let note = NoteEditor::new(props.folder_probe, window, cx);
        let folder = props
            .folder
            .clone()
            .filter(|_| props.place == Place::Window);
        let mut palette = Self {
            state: PaletteState::new(props.hooks.modes()).with_folder(folder),
            note,
            hooks: props.hooks,
            place: props.place,
            folder: props.folder,
            key_cap: props.key_cap,
            fixed_now: props.now_ms,
            width: props.width,
            now_ms: props.now_ms.unwrap_or_else(system_now),
            field,
            overlay: Overlay::default(),
            manage: None,
            form: None,
            repoint: None,
            intro,
            dormant: false,
            scroll: ListScroll::new(),
            _field: subscription,
        };
        palette.listen(cx);
        palette.refresh(false, cx);
        cx.on_release(|palette, cx| palette.flush_note(cx)).detach();
        // The flush runs as the observers are called, before any quit future stops a service.
        cx.on_app_quit(|palette, cx| {
            palette.flush_note(cx);
            std::future::ready(())
        })
        .detach();
        palette
    }

    pub fn query(&self, cx: &App) -> SharedString {
        self.field.read(cx).value()
    }

    pub fn mode(&self) -> ClipMode {
        self.state.mode()
    }

    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = query.to_owned();
        self.field
            .update(cx, |field, cx| field.set_value(text, window, cx));
        self.refresh(false, cx);
    }

    pub(super) fn reveal_selection(&self) {
        if let Some(position) = self.state.selected_entry() {
            self.scroll.scroll_to(position);
        }
    }

    /// Switches to a mode the hooks offer.
    pub fn pick_mode(&mut self, mode: ClipMode, cx: &mut Context<Self>) {
        if self.state.set_mode(mode) {
            self.refresh(false, cx);
            cx.notify();
        }
    }

    pub(super) fn cycle_mode(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if self.manage.is_some() {
            return self.manage_cycle(backwards, cx);
        }
        if self.close_actions(cx) {
            return;
        }
        if self.state.cycle(backwards) {
            self.refresh(false, cx);
            cx.notify();
        }
    }

    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.repoint = None;
        if self.manage.is_some() {
            return self.manage_step(delta, cx);
        }
        if self.step_actions(delta, cx) {
            return;
        }
        self.state.step(delta);
        self.reveal_selection();
        cx.notify();
    }

    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.is_some() {
            return self.form_back(window, cx);
        }
        if self.cancel_repoint(cx) {
            return;
        }
        if self.leave_note(window, cx) {
            return;
        }
        if self.manage.is_some() {
            return self.close_manage(cx);
        }
        if self.close_actions(cx) {
            return;
        }
        if self.query(cx).is_empty() {
            self.dismiss(cx);
        } else {
            self.set_query("", window, cx);
        }
    }
}

fn fade_in(panel: RowElement, theme: &Theme, cx: &App) -> AnyElement {
    use gpui_kit::base::animation::ease_out_cubic;
    use gpui_kit::{Animation, AnimationExt};
    if cx.reduce_motion() {
        return panel.into_any_element();
    }
    panel
        .with_animation(
            "clip-palette-enter",
            Animation::new(theme.motion.fade).with_easing(ease_out_cubic),
            |panel, delta| panel.opacity(delta),
        )
        .into_any_element()
}

impl ClipPalette {
    /// Selects the row at `index` in the shown results, if there is one.
    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.state.select(index);
        self.reveal_selection();
        cx.notify();
    }

    /// Alt+digit: Enter on the `n`th row in view (1-based). Does nothing without such a row,
    /// while a form, the manage view or the Ctrl+K menu is open, or in the note editor.
    pub(super) fn pick_row(&mut self, n: usize, window: &Window, cx: &mut Context<Self>) {
        let is_busy = self.manage.is_some() || self.form.is_some() || self.overlay.menu.is_some();
        if is_busy
            || self.note_has_focus(window, cx)
            || n == 0
            || n > self.state.results().rows.len()
        {
            return;
        }
        self.choose_at(n - 1, cx);
    }

    pub(super) fn choose_at(&mut self, index: usize, cx: &mut Context<Self>) {
        self.repoint = None;
        self.state.select(index);
        self.choose(cx);
    }

    /// The pop-up fills its window; in Navo's window the palette keeps a margin.
    fn frame(&self, window: &Window) -> (Pixels, Pixels) {
        let viewport = window.viewport_size();
        let margin = match self.place {
            Place::Window => px(MARGIN) * 2.,
            Place::Anywhere => px(0.),
        };
        let width = (self.width.unwrap_or(viewport.width) - margin).max(px(0.));
        let height = (viewport.height - margin).max(px(0.));
        (width.min(px(WIDTH)), height.min(px(HEIGHT)))
    }

    fn body(
        &self,
        t: &Theme,
        (is_docked, has_note_focus): (bool, bool),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_rows = !self.state.results().rows.is_empty();
        let paint = Paint::new(self, is_docked, cx.entity().downgrade());
        let list = div()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .border_color(t.colors.line)
            .p(t.spacing.xs)
            .when(is_docked, |list| list.flex_1().border_b_1())
            .when(!is_docked, |list| {
                list.flex_none().w(px(LIST_WIDTH)).border_r_1()
            })
            .child(if has_rows {
                paint
                    .list(&self.scroll, tr("clip-results-label"))
                    .into_any_element()
            } else {
                self.empty(t, cx).into_any_element()
            });
        let row = self.state.selected_row();
        let look = Look {
            now_ms: self.now_ms,
            is_docked,
            secret: row.and_then(|row| self.shown_secret(row)),
            has_note_focus,
        };
        let note = row.and_then(|row| self.note_pane(t, row, look));
        let dock = if note.is_some() {
            NOTE_DOCK_HEIGHT
        } else {
            DOCK_HEIGHT
        };
        let pane = note
            .unwrap_or_else(|| preview::preview(t, row, look))
            .when(is_docked, |pane| pane.flex_none().h(px(dock)));
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .when(is_docked, |body| body.flex_col())
            .child(list)
            .child(pane)
    }
}

impl Render for ClipPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.hide_stray_secret();
        self.sync_note(window, cx);
        let t = crate::tokens::current(cx).clone();
        let (width, height) = self.frame(window);
        let is_docked = width < px(DOCK_BELOW);
        let has_note_focus = self.note_has_focus(window, cx);
        let floats = self.place == Place::Window;
        let panel = div()
            .id(ROOT)
            .key_context(CONTEXT)
            .role(Role::Dialog)
            .aria_label(tr("clip-palette"))
            .test_support()
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .w(width)
            .h(height)
            .overflow_hidden()
            .when(floats, |panel| {
                panel.rounded(t.radii.sheet).shadow(t.shadows.menu.clone())
            })
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.sheet)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height);
        let panel =
            on_keys(panel, cx).on_mouse_down_out(cx.listener(|this, _, _, cx| this.dismiss(cx)));
        let panel = if self.form.is_some() {
            panel.children(self.form_view(&t, window, cx))
        } else if self.manage.is_some() {
            panel
                .child(self.input_row(&t))
                .child(self.manage_body(&t, is_docked, cx))
                .child(self.manage_footer(&t, cx))
        } else {
            panel
                .child(self.input_row(&t))
                .children(self.tabs(&t, is_docked, cx))
                .children(self.notices(&t))
                .child(self.body(&t, (is_docked, has_note_focus), cx))
                .child(self.footer(&t, cx))
                .children(self.actions_menu(&t, cx))
        };
        fade_in(panel.children(self.status_line(&t, is_docked, cx)), &t, cx)
    }
}
