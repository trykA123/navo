use rusty_ui::prelude::*;
use rusty_ui::primitives::{IconName, icon};
use rusty_ui::settings::COLOR_THEMES;
use rusty_ui::tokens::{ColorTheme, Mode, Theme, ThemeLook};

const CARD_WIDTH: f32 = 190.;
const DOT: f32 = 10.;

fn themed(color_theme: ColorTheme, mode: Mode) -> Theme {
    let look = ThemeLook {
        color_theme,
        ..ThemeLook::default()
    };
    Theme::with_look(mode, look)
}

fn line(t: &Theme, bg: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .h(t.spacing.row_height)
        .px(t.spacing.lg)
        .bg(bg)
}

fn name_row(t: &Theme, bg: Hsla, (color, glyph, name): (Hsla, IconName, &'static str)) -> Div {
    line(t, bg)
        .text_color(color)
        .child(icon(glyph, t.spacing.icon, color))
        .child(name)
}

fn kinds(t: &Theme) -> Div {
    let c = &t.colors;
    let dots = [
        c.kind_code,
        c.kind_image,
        c.kind_archive,
        c.kind_app,
        c.kind_pdf,
        c.kind_doc,
        c.kind_sheet,
        c.kind_slides,
        c.kind_audio,
        c.kind_video,
        c.kind_auto,
        c.ok,
        c.warn,
        c.mark,
    ];
    line(t, c.background).children(dots.into_iter().map(|color| {
        div()
            .flex_none()
            .size(px(DOT))
            .rounded(t.radii.full)
            .bg(color)
    }))
}

fn actions(t: &Theme) -> Div {
    let c = &t.colors;
    let button = div()
        .px(t.spacing.lg)
        .rounded(t.radii.md)
        .bg(c.accent)
        .text_color(c.background)
        .child("Copy");
    line(t, c.background)
        .child(button)
        .child(div().text_color(c.accent_text).child("Open link"))
        .child(div().text_color(c.faint).child("hint"))
}

fn files(t: &Theme) -> Div {
    let c = &t.colors;
    div()
        .flex()
        .flex_col()
        .child(name_row(
            t,
            c.background,
            (c.text, IconName::Folder, "Documents"),
        ))
        .child(name_row(t, c.zebra, (c.muted, IconName::File, "notes.txt")))
        .child(name_row(
            t,
            c.hover,
            (c.text, IconName::File, "hovered.txt"),
        ))
        .child(name_row(
            t,
            c.selection,
            (c.text, IconName::File, "selected.txt"),
        ))
        .child(name_row(
            t,
            c.background,
            (c.mark, IconName::File, "marked.txt"),
        ))
}

fn card(color_theme: ColorTheme, mode: Mode) -> Div {
    let t = themed(color_theme, mode);
    let c = &t.colors;
    let heading = match mode {
        Mode::Light => "Light",
        Mode::Dark => "Dark",
    };
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(CARD_WIDTH))
        .overflow_hidden()
        .rounded(t.radii.card)
        .border_1()
        .border_color(c.line)
        .bg(c.background)
        .text_size(t.typography.body)
        .child(line(&t, c.rail).text_color(c.muted).child(heading))
        .child(files(&t))
        .child(kinds(&t))
        .child(actions(&t))
        .child(
            line(&t, c.terminal_background)
                .font_family(t.typography.mono_font.clone())
                .text_color(c.terminal_text)
                .child("C:\\> dir"),
        )
}

fn family(t: &Theme, index: usize, color_theme: ColorTheme) -> Div {
    let name = COLOR_THEMES[index].text();
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .child(
            div()
                .text_size(t.typography.small)
                .font_weight(FontWeight::BOLD)
                .text_color(t.colors.text)
                .child(name),
        )
        .child(
            div()
                .flex()
                .gap(t.spacing.xl)
                .child(card(color_theme, Mode::Light))
                .child(card(color_theme, Mode::Dark)),
        )
}

pub fn render(t: &Theme) -> Div {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_wrap()
        .content_start()
        .gap(t.spacing.xxl)
        .p(t.spacing.xxl)
        .children(
            ColorTheme::ALL
                .into_iter()
                .enumerate()
                .map(|(index, color_theme)| family(t, index, color_theme)),
        )
}
