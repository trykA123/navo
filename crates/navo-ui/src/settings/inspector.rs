//! The Settings inspector: a column beside the rows that describes the focused setting
//! (Now, Default, help, pictures, Reset and its navo.toml key). On a small dialog it moves
//! under the rows.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role, canvas};

use super::controls::{self, Picks};
use super::fields::{Control, Field, FieldSpec, Look as ChoiceLook};
use super::fold;
use super::git_page;
use super::nav::NAV_WIDTH;
use super::row_state;
use super::values::{Value, Values};
use super::view::{Look, section_label, send_on_click, spoken};
use super::{HELP_ROW, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, Kbd};
use crate::tokens::{self, Theme};

/// Inspector width in px beside the rows (README `SET_INSP_W`).
pub(super) const WIDTH: f32 = 300.;
/// Dialog margins (2 × 24 px) and the nav: what a window loses before the rows.
const GUESS_TAKEN: f32 = 48. + NAV_WIDTH;
/// Rows plus inspector narrower than this (px) stack the inspector under the rows: a
/// 960 px window (README "Inspector") once the margins and the nav are taken off.
const STACK_BELOW: f32 = 960. - GUESS_TAKEN;
/// Share of the body height the stacked inspector may take.
const STACKED_SHARE: f32 = 0.34;
/// Width of the Now / Default label column (px).
const LABEL_W: f32 = 56.;
/// Gap between a value and its mark, or a button and its key cap (px).
const NEAR: f32 = 6.;

/// The inspector body for the focused row: the selected storage store, the setting's
/// details, the settings inside a fold row (Find empty), or `None` for a row the filter hides.
pub(super) fn details(look: &Look) -> Option<AnyElement> {
    if let Some(item) = selected_diagnostic(look) {
        return Some(super::diagnostics_page::inspector(look, item));
    }
    if fold::group_of_fold(look.row).is_some() {
        return look.filter.is_empty().then(|| fold_details(look, look.row));
    }
    focused(look).map(|(index, spec)| inspector(look, index, spec))
}

fn selected_diagnostic<'a>(look: &Look<'a>) -> Option<&'a super::DiagnosticRow> {
    if !look.diagnostics_open || super::section_of(look.row) != super::Section::About {
        return None;
    }
    look.selected_diagnostic
        .and_then(|index| look.diagnostics.get(index))
}

/// The focused row and its spec, if the focused row is a setting the filter shows.
fn focused(look: &Look) -> Option<(usize, &'static FieldSpec)> {
    if look.row == HELP_ROW {
        return Some((HELP_ROW, Field::CheckUpdates.spec()));
    }
    super::fields::FIELDS
        .get(look.row)
        .filter(|spec| look.filter.admits(spec))
        .map(|spec| (look.row, spec))
}

/// The rows and the inspector, side by side or stacked by the measured width of both.
#[derive(IntoElement)]
pub(super) struct Split {
    pub rows: AnyElement,
    pub inspector: Option<AnyElement>,
}

impl RenderOnce for Split {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let width = window.use_keyed_state("settings-split", cx, |window, _| {
            window.viewport_size().width - px(GUESS_TAKEN)
        });
        let is_stacked = *width.read(cx) < px(STACK_BELOW);
        let measure = canvas(
            move |bounds, window, cx| {
                let now = bounds.size.width;
                let flips = (now < px(STACK_BELOW)) != is_stacked;
                width.update(cx, |width, _| *width = now);
                if flips {
                    window.refresh();
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full();
        let inspector = self.inspector.map(|inner| frame(&t, is_stacked, inner));
        div()
            .relative()
            .flex()
            .when(is_stacked, |split| split.flex_col())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(measure)
            .child(self.rows)
            .children(inspector)
    }
}

/// The inspector's scrolling frame: a 300 px column beside the rows, or a band under them.
fn frame(t: &Theme, is_stacked: bool, inner: AnyElement) -> impl IntoElement + use<> {
    let frame = div()
        .id("settings-inspector")
        .role(Role::Region)
        .aria_label(navo_l10n::tr("settings-inspector-region"))
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .min_h_0()
        .overflow_y_scroll()
        .bg(t.surfaces.sheet)
        .border_color(t.colors.line);
    let frame = if is_stacked {
        frame.w_full().max_h(relative(STACKED_SHARE)).border_t_1()
    } else {
        frame.w(px(WIDTH)).border_l_1()
    };
    frame.child(inner)
}

/// The inspector body for the focused row `index` with `spec`.
fn inspector(look: &Look, index: usize, spec: &'static FieldSpec) -> AnyElement {
    let t = look.theme;
    let is_changed = look.values.is_changed(spec.field);
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xl)
        .p(t.spacing.xl)
        .child(eyebrow(t, spec))
        .child(
            div()
                .id("settings-inspector-name")
                .role(Role::Heading)
                .aria_label(spec.label())
                .test_support()
                .font_weight(FontWeight::SEMIBOLD)
                .child(spec.label()),
        )
        .child(state(look, spec, is_changed))
        .child(div().text_color(t.colors.text).child(spec.help()))
        .child(with_key(
            row_state::reset(index, spec.field, look.send.as_ref(), is_changed),
            "settings-reset-key",
            "f8",
        ))
        .child(toml(look, spec))
        // Pictures go last, so Reset and the key stay in view on a short dialog.
        .children(pictures(look, spec))
        .into_any_element()
}

/// The inspector on fold row `fold`: the group, how many settings it holds, and each one with
/// its value; a click opens the fold and selects that setting.
fn fold_details(look: &Look, fold: usize) -> AnyElement {
    let t = look.theme;
    let fields = fold::fold_fields(look.filter, fold);
    let heading = fold::count_text(fields.len());
    let items = fields.iter().map(|field| fold_item(look, *field));
    div()
        .id("settings-inspector-fold")
        .role(Role::Group)
        .aria_label(heading.clone())
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.xl)
        .p(t.spacing.xl)
        .children(fields.first().map(|field| eyebrow(t, field.spec())))
        .child(
            div()
                .id("settings-inspector-name")
                .role(Role::Heading)
                .aria_label(heading.clone())
                .test_support()
                .font_weight(FontWeight::SEMIBOLD)
                .child(heading),
        )
        .child(
            div()
                .text_color(t.colors.muted)
                .child(navo_l10n::tr("settings-fold-help")),
        )
        .child(div().flex().flex_col().gap(t.spacing.xxs).children(items))
        .into_any_element()
}

/// One setting in the fold inspector: its label and current value on one line.
fn fold_item(look: &Look, field: Field) -> AnyElement {
    let t = look.theme;
    let spec = field.spec();
    let value = shown_value(value_text(look, spec, look.values.get(field)));
    let item = div()
        .id(("settings-inspector-advanced", field.index()))
        .role(Role::Button)
        .aria_label(spec.label())
        .aria_value(value.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .rounded(t.radii.md)
        .cursor_pointer()
        .hover(|item| item.bg(t.colors.hover))
        .text_size(t.typography.small)
        .child(div().flex_1().min_w_0().truncate().child(spec.label()))
        .child(
            div()
                .flex_none()
                .max_w(relative(0.5))
                .truncate()
                .text_color(t.colors.muted)
                .child(value),
        );
    send_on_click(item, look.send.as_ref(), SheetEvent::Select(field.index())).into_any_element()
}

fn eyebrow(t: &Theme, spec: &FieldSpec) -> impl IntoElement + use<> {
    let page = section_label(spec.section);
    let text: SharedString = match super::pages::group_of(spec.field) {
        Some(group) => format!("{page} · {group}").into(),
        None => page,
    };
    div()
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.muted)
        .child(text)
}

/// The words for a value: the tool's name for the Git tool, else the spoken value.
fn value_text(look: &Look, spec: &FieldSpec, value: &Value) -> SharedString {
    if spec.control != Control::Tool {
        return spoken(spec, value);
    }
    let Value::Text(id) = value else {
        return SharedString::default();
    };
    look.git
        .options(id)
        .into_iter()
        .find(|option| *option.id == **id)
        .map_or_else(|| id.clone(), |option| option.label.into())
}

fn state(look: &Look, spec: &FieldSpec, is_changed: bool) -> impl IntoElement + use<> {
    let t = look.theme;
    let now = value_text(look, spec, look.values.get(spec.field));
    let mark = if is_changed {
        div()
            .flex()
            .items_center()
            .gap(t.spacing.xs)
            .text_color(t.colors.accent_text)
            .child(row_state::dot(t))
            .child("Changed")
    } else {
        div()
            .font_weight(FontWeight::NORMAL)
            .text_color(t.colors.muted)
            .child("· default")
    };
    let default = is_changed.then(|| {
        let shown = value_text(look, spec, Values::default().get(spec.field));
        line(
            t,
            "Default",
            value_cell("settings-inspector-default", shown, None),
        )
    });
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .text_size(t.typography.small)
        .child(line(
            t,
            "Now",
            value_cell("settings-inspector-now", now, Some(mark)),
        ))
        .children(default)
}

/// The words shown for a value: `text`, or "Not set" when it is empty.
fn shown_value(text: SharedString) -> SharedString {
    if text.is_empty() {
        navo_l10n::tr("settings-value-not-set").into()
    } else {
        text
    }
}

/// A value in the Now / Default lines, named `text` for assistive tech, with an optional mark.
fn value_cell(id: &'static str, text: SharedString, mark: Option<Div>) -> AnyElement {
    let shown = shown_value(text.clone());
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text)
        .test_support()
        .flex()
        .items_center()
        .gap(px(NEAR))
        .min_w_0()
        .font_weight(FontWeight::MEDIUM)
        .child(div().min_w_0().truncate().child(shown))
        .children(mark)
        .into_any_element()
}

pub(super) fn line(
    t: &Theme,
    name: impl Into<SharedString>,
    value: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .child(
            div()
                .flex_none()
                .w(px(LABEL_W))
                .text_color(t.colors.muted)
                .child(name.into()),
        )
        .child(value)
}

/// Pictures of the choices (theme, colour theme, density) or the Git tool list; clicking one
/// picks it.
fn pictures(look: &Look, spec: &FieldSpec) -> Option<AnyElement> {
    if spec.control == Control::Tool {
        let current = look.values.git_tool();
        return Some(git_page::tool_list(
            look.theme,
            look.git,
            current,
            look.send.as_ref(),
        ));
    }
    let (Control::Choices(options, choice_look), Value::Pick(at)) =
        (spec.control, look.values.get(spec.field))
    else {
        return None;
    };
    if choice_look == ChoiceLook::Segments {
        return None;
    }
    let picks = Picks {
        t: look.theme,
        field: spec.field,
        options,
        at: *at,
        send: look.send.as_ref(),
    };
    Some(controls::pictures(&picks, choice_look).into_any_element())
}

/// A button followed by the key cap of its shortcut.
fn with_key(button: impl IntoElement, id: &'static str, key: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(NEAR))
        .child(button)
        .children(Kbd::new(id, key))
}

fn toml(look: &Look, spec: &FieldSpec) -> impl IntoElement + use<> {
    let t = look.theme;
    let locate = Button::new("settings-locate", navo_l10n::tr("settings-open-line"))
        .variant(ButtonVariant::Ghost)
        .icon(IconName::FileCode)
        .tooltip(navo_l10n::tr("settings-open-line-tip"));
    let locate = match look.send.clone() {
        Some(send) => {
            let event = SheetEvent::Locate(spec.field);
            locate.on_click(move |_, window, cx| send(&event, window, cx))
        }
        None => locate,
    };
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .pt(t.spacing.xl)
        .border_t_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .child(
            div()
                .text_color(t.colors.muted)
                .child(navo_l10n::tr("settings-in-file")),
        )
        .child(
            div()
                .id("settings-key")
                .role(Role::Label)
                .aria_label(spec.key)
                .test_support()
                .font_family(t.typography.mono_font.clone())
                .text_color(t.colors.text)
                .child(spec.key),
        )
        .child(with_key(locate, "settings-locate-key", "f3"))
}
