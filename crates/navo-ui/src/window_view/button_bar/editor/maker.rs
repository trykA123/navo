//! Icon maker tab view: 1–2 letter badge builder, Lucide glyph+dot builder, and live preview.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_icons::spec::{Shape, TONES, Tone};
use navo_l10n::{t as tr_args, tr};

use super::editor_tab_button;
use super::maker_grid::glyph_grid_area;
use super::maker_state::{MakerMode, MakerState, shape_label, tone_label};
use super::picker::PickerState;
use crate::primitives::{Button, ButtonVariant, TextField, Tooltip};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::BarIcon;
use crate::window_view::button_bar::icon::{icon_element, letter_colours, tone_colour};

const SWATCH_SIZE: f32 = 22.;
const SWATCH_MARK_SIZE: f32 = 6.;
const PREVIEW_SIZES: [(f32, &str); 3] = [
    (16., "button-bar-maker-preview-16"),
    (28., "button-bar-maker-preview-28"),
    (48., "button-bar-maker-preview-48"),
];
const SHAPES: [(Shape, &str); 5] = [
    (Shape::Soft, "button-bar-maker-shape-soft"),
    (Shape::Circle, "button-bar-maker-shape-circle"),
    (Shape::Outline, "button-bar-maker-shape-outline"),
    (Shape::Solid, "button-bar-maker-shape-solid"),
    (Shape::None, "button-bar-maker-shape-none"),
];

struct ToneRow {
    prefix: &'static str,
    label: String,
    selected: Option<Tone>,
    allow_none: bool,
}

impl NavoWindow {
    /// Renders the Maker tab body (Letters or Glyph controls, 16/28/48 px preview, and Use button).
    pub(super) fn maker_body(
        &self,
        picker: &PickerState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let maker = &picker.maker;
        let controls = match maker.mode {
            MakerMode::Letters => self.maker_letters_panel(maker, t, cx),
            MakerMode::Glyph => self.maker_glyph_panel(maker, !picker.has_index(), t, cx),
        };
        div()
            .id("button-bar-maker")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(self.maker_mode_tabs(maker.mode, t, cx))
            .child(controls)
            .child(self.maker_footer(maker, t, cx))
            .into_any_element()
    }

    fn maker_mode_tabs(&self, active: MakerMode, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let letters_tab = editor_tab_button(
            "button-bar-maker-mode-letters",
            tr("button-bar-maker-mode-letters"),
            active == MakerMode::Letters,
            t,
        )
        .on_click(cx.listener(|view, _, _, cx| view.set_maker_mode(MakerMode::Letters, cx)));

        let glyph_tab = editor_tab_button(
            "button-bar-maker-mode-glyph",
            tr("button-bar-maker-mode-glyph"),
            active == MakerMode::Glyph,
            t,
        )
        .on_click(cx.listener(|view, _, _, cx| view.set_maker_mode(MakerMode::Glyph, cx)));

        div()
            .id("button-bar-maker-mode-tabs")
            .role(Role::TabList)
            .aria_label(tr("button-bar-maker-mode-label"))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .child(letters_tab)
            .child(glyph_tab)
            .into_any_element()
    }

    fn maker_letters_panel(
        &self,
        maker: &MakerState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let invalid = maker.current_spec(cx).is_none();
        div()
            .id("button-bar-maker-letters-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.md)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap(t.spacing.xs)
                    .child(section_label(tr("button-bar-maker-letters"), t))
                    .child(TextField::new(
                        "button-bar-maker-letters",
                        &maker.letters_input,
                        tr("button-bar-maker-letters"),
                    ))
                    .when(invalid, |col| {
                        col.child(
                            div()
                                .text_size(t.typography.small)
                                .text_color(t.colors.muted)
                                .child(tr("button-bar-maker-letters-hint")),
                        )
                    }),
            )
            .child(self.maker_shapes_row(maker.shape, t, cx))
            .child(self.maker_tone_section(
                ToneRow {
                    prefix: "button-bar-maker-tone",
                    label: tr("button-bar-maker-tone"),
                    selected: Some(maker.letter_tone),
                    allow_none: false,
                },
                t,
                cx,
            ))
            .into_any_element()
    }

    fn maker_shapes_row(&self, active: Shape, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .id("button-bar-maker-shapes")
            .role(Role::RadioGroup)
            .aria_label(tr("button-bar-maker-shape"))
            .test_support()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.xs);

        for (shape, id) in SHAPES {
            let btn = editor_tab_button(id, shape_label(shape), active == shape, t)
                .on_click(cx.listener(move |view, _, _, cx| view.set_maker_shape(shape, cx)));
            row = row.child(btn);
        }

        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(t.spacing.xs)
            .child(section_label(tr("button-bar-maker-shape"), t))
            .child(row)
            .into_any_element()
    }

    fn maker_glyph_panel(
        &self,
        maker: &MakerState,
        loading: bool,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let grid = glyph_grid_area(
            Rc::clone(&maker.glyph_rows),
            &maker.glyph_scroll,
            &maker.glyph_name,
            loading,
            t,
            cx,
        );
        div()
            .id("button-bar-maker-glyph-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(TextField::new(
                "button-bar-maker-glyph-search",
                &maker.glyph_search,
                tr("button-bar-maker-glyph-search"),
            ))
            .child(grid)
            .child(self.maker_tone_section(
                ToneRow {
                    prefix: "button-bar-maker-glyph-tone",
                    label: tr("button-bar-maker-tone"),
                    selected: Some(maker.glyph_tone),
                    allow_none: false,
                },
                t,
                cx,
            ))
            .child(self.maker_tone_section(
                ToneRow {
                    prefix: "button-bar-maker-dot",
                    label: tr("button-bar-maker-dot"),
                    selected: maker.glyph_dot,
                    allow_none: true,
                },
                t,
                cx,
            ))
            .into_any_element()
    }

    fn maker_tone_section(&self, spec: ToneRow, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.xs);

        if spec.allow_none {
            let none_btn = editor_tab_button(
                "button-bar-maker-dot-none",
                tr("button-bar-maker-dot-none"),
                spec.selected.is_none(),
                t,
            )
            .on_click(cx.listener(|view, _, _, cx| view.set_maker_glyph_dot(None, cx)));
            row = row.child(none_btn);
        }

        let prefix = spec.prefix;
        for (idx, tone) in TONES.iter().copied().enumerate() {
            let is_selected = spec.selected == Some(tone);
            let swatch = tone_swatch(prefix, idx, tone, is_selected, t).on_click(cx.listener(
                move |view, _, _, cx| match prefix {
                    "button-bar-maker-tone" => view.set_maker_letter_tone(tone, cx),
                    "button-bar-maker-glyph-tone" => view.set_maker_glyph_tone(tone, cx),
                    _ => view.set_maker_glyph_dot(Some(tone), cx),
                },
            ));
            row = row.child(swatch);
        }

        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(t.spacing.xs)
            .child(section_label(spec.label, t))
            .child(row)
            .into_any_element()
    }

    fn maker_footer(&self, maker: &MakerState, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let spec = maker.current_spec(cx);
        let preview = maker.preview_icon(cx);
        let mut previews = div()
            .id("button-bar-maker-preview")
            .test_support()
            .flex()
            .items_end()
            .gap(t.spacing.md);

        for (side, id) in PREVIEW_SIZES {
            previews = previews.child(preview_box(id, side, &preview, t, cx));
        }

        let use_button = Button::new("button-bar-maker-use", tr("button-bar-maker-use"))
            .variant(ButtonVariant::Primary)
            .disabled(spec.is_none())
            .on_click(cx.listener(|view, _, window, cx| view.use_maker_icon(window, cx)));

        div()
            .flex()
            .flex_none()
            .items_end()
            .justify_between()
            .gap(t.spacing.md)
            .pt(t.spacing.sm)
            .border_t_1()
            .border_color(t.surfaces.card_edge)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.xs)
                    .child(section_label(tr("button-bar-maker-preview"), t))
                    .child(previews),
            )
            .child(use_button)
            .into_any_element()
    }

    pub(super) fn maker_letters_changed(&mut self, cx: &mut Context<Self>) {
        if self.button_bar.editor.is_some() {
            cx.notify();
        }
    }

    pub(super) fn maker_glyph_search_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.refresh_maker_glyphs(cx);
            cx.notify();
        }
    }

    fn set_maker_mode(&mut self, mode: MakerMode, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut()
            && editor.picker.maker.mode != mode
        {
            editor.picker.maker.mode = mode;
            cx.notify();
        }
    }

    fn set_maker_shape(&mut self, shape: Shape, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.maker.shape = shape;
            cx.notify();
        }
    }

    fn set_maker_letter_tone(&mut self, tone: Tone, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.maker.letter_tone = tone;
            cx.notify();
        }
    }

    pub(super) fn set_maker_glyph(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.maker.glyph_name = name;
            cx.notify();
        }
    }

    fn set_maker_glyph_tone(&mut self, tone: Tone, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.maker.glyph_tone = tone;
            cx.notify();
        }
    }

    fn set_maker_glyph_dot(&mut self, dot: Option<Tone>, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.maker.glyph_dot = dot;
            cx.notify();
        }
    }

    fn use_maker_icon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(spec) = self
            .button_bar
            .editor
            .as_ref()
            .and_then(|editor| editor.picker.maker.current_spec(cx))
        else {
            return;
        };
        self.pick_editor_icon(spec, window, cx);
    }
}

fn tone_swatch(
    prefix: &'static str,
    idx: usize,
    tone: Tone,
    selected: bool,
    t: &Theme,
) -> gpui_kit::base::ObservedElement<gpui_kit::Stateful<Div>> {
    let swatch = tone_colour(tone, &t.colors);
    let (_, mark_colour) = letter_colours(Shape::Solid, swatch, &t.colors);
    let label = tone_label(tone);
    let tip: SharedString = label.clone().into();
    div()
        .id((prefix, idx))
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(selected)
        .test_support()
        .size(px(SWATCH_SIZE))
        .rounded(t.radii.full)
        .bg(swatch)
        .border_2()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(selected, |d| d.border_color(t.colors.text))
        .when(!selected, |d| d.border_color(t.surfaces.card_edge))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .when(selected, |d| {
            d.child(
                div()
                    .size(px(SWATCH_MARK_SIZE))
                    .rounded(t.radii.full)
                    .bg(mark_colour),
            )
        })
}

fn preview_box(id: &'static str, side: f32, icon: &BarIcon, t: &Theme, cx: &App) -> AnyElement {
    let label = tr_args!("button-bar-maker-preview-size", size = side as u32);
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.spacing.xxs)
        .child(
            div()
                .id(id)
                .test_support()
                .p(t.spacing.xs)
                .rounded(t.radii.sm)
                .border_1()
                .border_color(t.surfaces.card_edge)
                .bg(t.colors.rail)
                .flex()
                .items_center()
                .justify_center()
                .child(icon_element(icon, px(side), cx)),
        )
        .child(
            div()
                .text_size(t.typography.tiny)
                .text_color(t.colors.muted)
                .child(label),
        )
        .into_any_element()
}

fn section_label(text: String, t: &Theme) -> Div {
    div()
        .text_size(t.typography.small)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.muted)
        .child(text)
}
