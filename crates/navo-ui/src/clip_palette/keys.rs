use gpui_kit::{Action, App, Context, InteractiveElement, Window};

use super::note_editor::Pass;
use super::rows::RowElement;
use super::view::ClipPalette;

pub(super) const CONTEXT: &str = "ClipPalette";

gpui_kit::actions!(
    clip_palette,
    [
        NextMode,
        PreviousMode,
        MoveUp,
        MoveDown,
        Choose,
        Back,
        OpenActions,
        CopyOnly,
        PasteFiles,
        ToggleReveal,
        CopyUser,
        SaveRow,
        DeleteRow,
        OpenManage,
        EditNote,
        NewItem,
        SaveForm,
        PreviousChoice,
        NextChoice,
        PickRow1,
        PickRow2,
        PickRow3,
        PickRow4,
        PickRow5,
        PickRow6,
        PickRow7,
        PickRow8,
        PickRow9
    ]
);

/// Binds the palette's keys from the keymap.
pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "clip_palette");
}

type Run = fn(&mut ClipPalette, &mut Window, &mut Context<ClipPalette>);

/// Runs a palette action, unless the note editor has focus: then its own key does the work.
fn passing<A: Action>(
    panel: RowElement,
    cx: &mut Context<ClipPalette>,
    (key, run): (Pass, Run),
) -> RowElement {
    panel.on_action(cx.listener(move |this, _: &A, window, cx| {
        if !this.form_key(key, window, cx) && !this.pass_to_note(key, window, cx) {
            run(this, window, cx);
        }
    }))
}

/// The keys of the saved item form: New, Save, and Left and Right on the chips.
fn on_form_keys(panel: RowElement, cx: &mut Context<ClipPalette>) -> RowElement {
    panel
        .on_action(cx.listener(|this, _: &NewItem, window, cx| this.open_new(window, cx)))
        .on_action(cx.listener(|this, _: &SaveForm, window, cx| this.save_form(window, cx)))
        .on_action(
            cx.listener(|this, _: &PreviousChoice, window, cx| this.step_choice(-1, window, cx)),
        )
        .on_action(cx.listener(|this, _: &NextChoice, window, cx| this.step_choice(1, window, cx)))
}

/// Alt+1 to Alt+9: Enter on the 1st to 9th row in view.
fn on_pick_keys(panel: RowElement, cx: &mut Context<ClipPalette>) -> RowElement {
    macro_rules! pick {
        ($panel:expr, $($action:ident => $n:expr),*) => {
            $panel$(.on_action(cx.listener(|this, _: &$action, window, cx| {
                this.pick_row($n, window, cx)
            })))*
        };
    }
    pick!(
        panel,
        PickRow1 => 1, PickRow2 => 2, PickRow3 => 3, PickRow4 => 4, PickRow5 => 5,
        PickRow6 => 6, PickRow7 => 7, PickRow8 => 8, PickRow9 => 9
    )
}

pub(super) fn on_keys(panel: RowElement, cx: &mut Context<ClipPalette>) -> RowElement {
    let panel = on_pick_keys(on_form_keys(panel, cx), cx);
    let panel = passing::<NextMode>(
        panel,
        cx,
        (Pass::Indent, |this, _, cx| {
            this.cycle_mode(false, cx);
        }),
    );
    let panel = passing::<PreviousMode>(
        panel,
        cx,
        (Pass::Outdent, |this, _, cx| {
            this.cycle_mode(true, cx);
        }),
    );
    let panel = passing::<MoveUp>(panel, cx, (Pass::Up, |this, _, cx| this.step(-1, cx)));
    let panel = passing::<MoveDown>(panel, cx, (Pass::Down, |this, _, cx| this.step(1, cx)));
    let panel = passing::<Choose>(panel, cx, (Pass::Enter, |this, _, cx| this.choose(cx)));
    let panel = passing::<CopyOnly>(
        panel,
        cx,
        (Pass::Copy, |this, window, cx| {
            this.copy_only(window, cx);
        }),
    );
    let panel = passing::<PasteFiles>(
        panel,
        cx,
        (Pass::Paste, |this, window, cx| {
            this.paste_files(window, cx);
        }),
    );
    let panel = passing::<DeleteRow>(
        panel,
        cx,
        (Pass::Delete, |this, window, cx| {
            this.delete_row(window, cx);
        }),
    );
    panel
        .on_action(cx.listener(|this, _: &Back, window, cx| this.back(window, cx)))
        .on_action(cx.listener(|this, _: &OpenActions, _, cx| this.toggle_actions(cx)))
        .on_action(cx.listener(|this, _: &ToggleReveal, _, cx| this.toggle_reveal(cx)))
        .on_action(cx.listener(|this, _: &CopyUser, _, cx| this.copy_user_key(cx)))
        .on_action(cx.listener(|this, _: &SaveRow, window, cx| this.save_key(window, cx)))
        .on_action(cx.listener(|this, _: &OpenManage, window, cx| this.edit_selected(window, cx)))
        .on_action(cx.listener(|this, _: &EditNote, _, cx| this.focus_note(cx)))
}
