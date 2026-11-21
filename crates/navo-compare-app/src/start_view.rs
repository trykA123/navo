//! Drawing of the start screen: the two fields with their chips, the buttons, the hint line.
//! The rows below them come from `start_rows`.

use gpui_kit::assets::IconName;
use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::{Button, ButtonVariant, IconButton, Kbd, TextField};
use sidewise_kit::tokens;
use sidewise_l10n::t;

use crate::menu_host::menu_view;
use crate::start::{
    CONTEXT, EnterList, OpenSelected, OtherField, PrevField, SelectNext, SelectPrev, Side,
    SideHint, StartScreen,
};
use crate::start_rows::{row_count, rows_view};
use crate::start_tabs::{StartMode, StartTabs, hero, mode_tabs, rule_chip};
use crate::theme_data::palettes::DIMS;

/// The chip beside a field: "Path", "Git revision", or a problem such as "Not found" (problems in the warning colour).
fn chip(hint: &SideHint, cx: &App) -> Option<Div> {
    let t = tokens::current(cx);
    let (label, color) = match hint {
        SideHint::Path(_) => ("Path".to_string(), t.colors.muted),
        SideHint::GitRef(_) => (t!("git-chip-revision"), t.colors.muted),
        SideHint::GitProblem(p) => (t!(p.l10n_id()), t.colors.warn),
        SideHint::Remote(_) => (t!("git-remote-chip-revision"), t.colors.muted),
        SideHint::BadAddress => (t!("git-remote-chip-bad-address"), t.colors.warn),
        SideHint::NotFound => ("Not found".to_string(), t.colors.warn),
        SideHint::Empty | SideHint::Checking => return None,
    };
    Some(
        div()
            .flex_none()
            .px(t.spacing.md)
            .py(t.spacing.xxs)
            .rounded(t.radii.pill)
            .border_1()
            .border_color(color)
            .text_size(t.typography.tiny)
            .text_color(color)
            .child(label.clone().test_text(label)),
    )
}

impl StartScreen {
    /// One side: its label, the text field and the chip.
    fn field_view(&self, side: Side, mode: StartMode, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        let id = match side {
            Side::Left => "start-left",
            Side::Right => "start-right",
        };
        let label = mode.field_label(side);
        let i = side.index();
        div()
            .flex()
            .flex_1()
            .flex_wrap()
            .min_w(DIMS.start_field_min_w)
            .items_center()
            .gap(t.spacing.md)
            .child(
                div()
                    .flex_none()
                    .text_size(t.typography.small)
                    .text_color(t.colors.faint)
                    .child(label.test_text(label)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(DIMS.start_field_min_w / 2.)
                    .child(TextField::new(id, &self.fields[i], label)),
            )
            .children(chip(&self.hints[i], cx))
    }

    /// The row of both fields with the swap and Compare buttons.
    fn fields_row(
        &self,
        mode: StartMode,
        tabs: &Entity<StartTabs>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let t = tokens::current(cx);
        let swap = IconButton::new("start-swap", IconName::Replace, "Swap sides")
            .on_click(cx.listener(|this, _, window, cx| this.swap(window, cx)));
        let compare = Button::new("start-compare", "Compare")
            .track_focus(&tabs.read(cx).compare)
            .variant(ButtonVariant::Primary)
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.md)
            .w_full()
            .child(self.field_view(Side::Left, mode, cx))
            .child(swap)
            .child(self.field_view(Side::Right, mode, cx))
            .child(compare)
    }

    /// The one-line tip about `repo@rev` sides; the Folder mode shows it.
    fn git_tip(&self, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .id("start-git-tip")
            .test_support()
            .px(t.spacing.xs)
            .text_size(t.typography.tiny)
            .text_color(t.colors.muted)
            .child(t!("git-start-tip"))
    }

    /// The muted line under the fields, with the `Tab` and `Enter` hints.
    fn hint_line(&self, cx: &App) -> impl IntoElement {
        let t = tokens::current(cx);
        let key = |id: &'static str, keys: &str, label: &'static str| {
            Kbd::new(id, keys).map(|kbd| {
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.sm)
                    .child(kbd)
                    .child(label)
            })
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.lg)
            .px(t.spacing.xs)
            .py(t.spacing.xs)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child("Type or paste a path.")
            .children(key("hint-tab", "tab", "next side"))
            .children(key("hint-enter", "enter", "compare"))
    }
}

impl Render for StartScreen {
    /// Draws the tabs, the fields, the rule chip, the hint line, the hero and the rows inside the `Start` context.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self.tabs.clone();
        let t = tokens::current(cx);
        let selected = self.selected.filter(|_| self.list_focus.is_focused(window));
        let rows = rows_view(
            &self.rows,
            selected,
            (&self.list_focus, &self.row_bounds),
            cx,
        );
        let choice = tabs.read(cx).choice();
        div()
            .key_context(CONTEXT)
            .on_action(
                cx.listener(|this, _: &OtherField, window, cx| this.cycle_stop(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PrevField, window, cx| this.cycle_stop(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &EnterList, window, cx| this.enter_list(window, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_next(cx)))
            .on_action(cx.listener(|this, _: &SelectPrev, window, cx| this.select_prev(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSelected, _, cx| this.open_selected(cx)))
            .flex()
            .flex_col()
            .size_full()
            .gap(t.spacing.sm)
            .px(t.spacing.xxl * 4.)
            .pt(t.spacing.xxl * 4.)
            .child(mode_tabs(&tabs, choice.mode, cx))
            .child(self.fields_row(choice.mode, &tabs, cx))
            .when(choice.mode == StartMode::Folder, |col| {
                col.child(self.git_tip(cx))
            })
            .child(rule_chip(&tabs, choice.respect_gitignore, cx))
            .child(self.hint_line(cx))
            .when(row_count(&self.rows) == 0, |col| col.child(hero(cx)))
            .child(rows)
            .child(self.updates.clone())
            .children(menu_view(&self.menu))
    }
}

#[cfg(test)]
#[path = "start_narrow_tests.rs"]
mod start_narrow_tests;
