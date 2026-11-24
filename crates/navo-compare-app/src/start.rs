//! The start screen: two side fields with live hints, the Compare button and the recent rows.
//! It checks typed sides off the UI thread (NS12) and emits events; the launch atom listens later.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{
    AppContext as _, EventEmitter, FocusHandle, Focusable as _, KeyBinding, Subscription, Task,
};
use navo_compare_cli::args::SideSpec;
use navo_compare_profile::recent::StartRows;
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::{TextFieldState, text_field_state};

use crate::menu_host::MenuSlot;
use crate::start_git::probe;
use crate::start_menu::{CopyPaths, ForgetSelected, ShowRowMenu};
use crate::start_rows::{LIST_CONTEXT, RowBounds, row_at, row_count};
use crate::start_tabs::{StartChoice, StartMode, StartTabs};
use crate::start_updates::UpdateLine;

gpui_kit::actions!(
    start,
    [
        OtherField,
        PrevField,
        EnterList,
        SelectNext,
        SelectPrev,
        OpenSelected
    ]
);

/// Key context of the screen's root element.
pub(crate) const CONTEXT: &str = "Start";

/// What the start screen knows about one typed side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideHint {
    Empty,
    Checking,
    Path(SideSpec),
    GitRef(SideSpec),
    GitProblem(sidewise_git::repo::GitProblem),
    /// A well-formed remote address; nothing is fetched until Compare.
    Remote(SideSpec),
    /// Text that looks like an address but is refused.
    BadAddress,
    NotFound,
}

/// What the start screen asks for: compare two sides, or open a stored row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartEvent {
    Compare {
        left: SideSpec,
        right: SideSpec,
        choice: StartChoice,
    },
    Open {
        id: String,
    },
    /// Remove the unnamed stored row `id` from Recent.
    Forget {
        id: String,
    },
}

/// One of the two side fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// Position of the side in the screen's two-element arrays.
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

/// Binds the start screen's keys; the shell calls it from its own `bind_keys`.
/// The field takes a plain `down`, so the field key is bound to the field inside the screen.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", OtherField, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PrevField, Some(CONTEXT)),
        KeyBinding::new("down", EnterList, Some("Start > Input")),
        KeyBinding::new("down", SelectNext, Some(LIST_CONTEXT)),
        KeyBinding::new("up", SelectPrev, Some(LIST_CONTEXT)),
        KeyBinding::new("enter", OpenSelected, Some(LIST_CONTEXT)),
        KeyBinding::new("ctrl-shift-c", CopyPaths, Some(LIST_CONTEXT)),
        KeyBinding::new("delete", ForgetSelected, Some(LIST_CONTEXT)),
        KeyBinding::new("shift-f10", ShowRowMenu, Some(LIST_CONTEXT)),
    ]);
}

/// The side a good hint stands for: a path that exists or a git revision that resolved.
fn good(hint: &SideHint) -> Option<&SideSpec> {
    match hint {
        SideHint::Path(spec) | SideHint::GitRef(spec) | SideHint::Remote(spec) => Some(spec),
        _ => None,
    }
}

/// The start screen view: two fields, their hints, the stored rows and the row selection.
pub struct StartScreen {
    pub(crate) fields: [Entity<TextFieldState>; 2],
    pub(crate) hints: [SideHint; 2],
    pub(crate) tabs: Entity<StartTabs>,
    /// The update line at the foot of the screen.
    pub(crate) updates: Entity<UpdateLine>,
    checks: [Option<Task<()>>; 2],
    pub(crate) rows: StartRows,
    pub(crate) selected: Option<usize>,
    cwd: PathBuf,
    pub(crate) list_focus: FocusHandle,
    pub(crate) menu: MenuSlot,
    /// Where the selected row was last drawn; the menu key anchors to it.
    pub(crate) row_bounds: RowBounds,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<StartEvent> for StartScreen {}

impl StartScreen {
    /// Makes a screen over `rows`; relative sides resolve against `cwd`.
    pub fn new(rows: StartRows, cwd: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hold = StartMode::default().placeholder();
        let left = text_field_state(hold, window, cx);
        let right = text_field_state(hold, window, cx);
        let tabs = cx.new(StartTabs::new);
        let on_tabs = cx.observe_in(&tabs, window, |this, tabs, window, cx| {
            let hold = tabs.read(cx).choice().mode.placeholder();
            for field in &this.fields {
                field.update(cx, |f, cx| f.set_placeholder(hold, window, cx));
            }
            cx.notify();
        });
        let subscriptions = [(Side::Left, &left), (Side::Right, &right)]
            .into_iter()
            .map(|(side, field)| {
                cx.subscribe_in(
                    field,
                    window,
                    move |this, _, event: &InputEvent, window, cx| match event {
                        InputEvent::Change => this.check(side, cx),
                        InputEvent::PressEnter { .. } => this.submit(window, cx),
                        _ => {}
                    },
                )
            })
            .chain([on_tabs])
            .collect();
        Self {
            fields: [left, right],
            hints: [SideHint::Empty, SideHint::Empty],
            tabs,
            updates: cx.new(UpdateLine::new),
            checks: [None, None],
            rows,
            selected: None,
            cwd,
            list_focus: cx.focus_handle(),
            menu: None,
            row_bounds: Rc::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Replaces both texts and checks both sides.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the launch atom sets sides; the view tests use it now"
        )
    )]
    pub fn set_sides(
        &mut self,
        left: &str,
        right: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (side, text) in [(Side::Left, left), (Side::Right, right)] {
            self.set_text((side, text), window, cx);
            self.check(side, cx);
        }
    }

    /// The hints of the left and the right side.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the view tests"))]
    pub fn hints(&self) -> (&SideHint, &SideHint) {
        (&self.hints[0], &self.hints[1])
    }

    /// Exchanges the two texts and the two hints; a side still checking is checked again.
    pub fn swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let texts = self.fields.each_ref().map(|field| field.read(cx).value());
        self.set_text((Side::Left, &texts[1]), window, cx);
        self.set_text((Side::Right, &texts[0]), window, cx);
        self.hints.swap(0, 1);
        self.checks = [None, None];
        for side in [Side::Left, Side::Right] {
            if self.hints[side.index()] == SideHint::Checking {
                self.check(side, cx);
            }
        }
        cx.notify();
    }

    /// Emits `Compare` when both hints are good; else focuses the first side that is not.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(left), Some(right)) = (good(&self.hints[0]), good(&self.hints[1])) {
            let event = StartEvent::Compare {
                left: left.clone(),
                right: right.clone(),
                choice: self.tabs.read(cx).choice(),
            };
            cx.emit(event);
            return;
        }
        let side = if good(&self.hints[0]).is_none() {
            Side::Left
        } else {
            Side::Right
        };
        self.focus_side(side, window, cx);
    }

    /// Emits `Open` for the stored row `id`; an id that is not in the rows does nothing.
    pub fn open_row(&mut self, id: &str, cx: &mut Context<Self>) {
        let known = self
            .rows
            .recent
            .iter()
            .chain(self.rows.named.iter())
            .any(|row| row.id == id);
        if known {
            cx.emit(StartEvent::Open { id: id.to_owned() });
        }
    }

    /// A click on the row at flat position `index`: only selects it (a double click opens, NC135).
    /// An index past the last row does nothing.
    pub(crate) fn click_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= row_count(&self.rows) {
            return;
        }
        self.selected = Some(index);
        cx.notify();
    }

    /// Puts the focus on the left field (NS16).
    pub fn focus_left(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_side(Side::Left, window, cx);
    }

    /// Which field has the focus, if any.
    pub fn focused_side(&self, window: &Window, cx: &App) -> Option<Side> {
        [Side::Left, Side::Right].into_iter().find(|side| {
            self.fields[side.index()]
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        })
    }

    /// Focuses one field.
    pub(crate) fn focus_side(&self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        self.fields[side.index()].update(cx, |field, cx| field.focus(window, cx));
    }

    /// Sets a field's text; this emits no change event, so the caller checks the side.
    fn set_text(&self, (side, text): (Side, &str), window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_owned();
        self.fields[side.index()].update(cx, |field, cx| field.set_value(text, window, cx));
    }

    /// Follows a field's text: empty text is `Empty`, any other text is `Checking` until the
    /// background check ends. A newer check replaces (cancels) the older one.
    fn check(&mut self, side: Side, cx: &mut Context<Self>) {
        let i = side.index();
        let text = self.fields[i].read(cx).value().to_string();
        cx.notify();
        if text.trim().is_empty() {
            self.hints[i] = SideHint::Empty;
            self.checks[i] = None;
            return;
        }
        self.hints[i] = SideHint::Checking;
        let cwd = self.cwd.clone();
        self.checks[i] = Some(cx.spawn(async move |this, cx| {
            let typed = text.clone();
            let hint = cx
                .background_spawn(async move { probe(typed.trim(), &cwd) })
                .await;
            this.update(cx, |this, cx| this.finish_check((side, &text), hint, cx))
                .ok();
        }));
    }

    /// Stores a check result only when the field still holds the checked text.
    fn finish_check(
        &mut self,
        (side, checked): (Side, &str),
        hint: SideHint,
        cx: &mut Context<Self>,
    ) {
        let now = self.fields[side.index()].read(cx).value();
        if &*now == checked {
            self.hints[side.index()] = hint;
            cx.notify();
        }
    }

    /// Down in a field: the list gets the focus and its first row is selected.
    pub(crate) fn enter_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if row_count(&self.rows) == 0 {
            return;
        }
        self.selected = Some(0);
        self.list_focus.focus(window, cx);
        cx.notify();
    }

    /// Down in the list: the next row; the last row stays selected.
    pub(crate) fn select_next(&mut self, cx: &mut Context<Self>) {
        let last = row_count(&self.rows).saturating_sub(1);
        self.selected = self.selected.map(|row| (row + 1).min(last));
        cx.notify();
    }

    /// Up in the list: the previous row; up on the first row returns to the left field.
    pub(crate) fn select_prev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.selected {
            Some(row) if row > 0 => self.selected = Some(row - 1),
            _ => {
                self.selected = None;
                self.focus_left(window, cx);
            }
        }
        cx.notify();
    }

    /// Enter in the list: opens the selected row.
    pub(crate) fn open_selected(&mut self, cx: &mut Context<Self>) {
        let id = self
            .selected
            .and_then(|row| row_at(&self.rows, row))
            .map(|row| row.id.clone());
        if let Some(id) = id {
            self.open_row(&id, cx);
        }
    }
}
