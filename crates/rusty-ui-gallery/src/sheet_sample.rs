use rusty_ui::prelude::*;
use rusty_ui::primitives::{IconName, icon};
use rusty_ui::side_sheet::{SheetFrame, SheetState, SideSheet};
use rusty_ui::tokens::Theme;

const ICON_BOX: f32 = 32.;
const STATS: [(&str, &str); 4] = [
    ("Size", "38.4 MB"),
    ("Files", "1,902"),
    ("Folders", "214"),
    ("Changed", "2026-10-16 09:12"),
];
const LARGEST: [(&str, &str); 5] = [
    (r"target\debug\ferrite.exe", "41.2 MB"),
    (r"target\debug\ferrite.pdb", "33.0 MB"),
    (r"rusty-ui\assets\fonts.bin", "4.1 MB"),
    (r"rusty-core\tests\fixtures.zip", "2.6 MB"),
    (r"rusty-fs\benches\tree.json", "1.3 MB"),
];

fn title(t: &Theme) -> Div {
    let mark = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(ICON_BOX))
        .rounded(t.radii.card)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .child(icon(IconName::Folder, t.spacing.icon, t.colors.folder));
    let text = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child("crates"),
        )
        .child(
            div()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(r"D:\work\ferrite\crates"),
        );
    let kind = div()
        .flex_none()
        .px(t.spacing.md)
        .rounded(t.radii.pill)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child("Folder");
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(t.spacing.lg)
        .child(mark)
        .child(text)
        .child(kind)
}

fn stat(t: &Theme, (label, value): (&'static str, &'static str)) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .py(t.spacing.lg)
        .px(t.spacing.xl)
        .rounded(t.radii.card)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(label),
        )
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
}

fn largest(t: &Theme, (path, size): (&'static str, &'static str)) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(t.rows.height)
        .child(icon(IconName::FileText, t.spacing.icon, t.colors.muted))
        .child(div().flex_1().min_w_0().truncate().child(path))
        .child(div().flex_none().text_color(t.colors.muted).child(size))
}

fn heading(t: &Theme, text: &'static str) -> Div {
    div()
        .pt(t.spacing.xl)
        .pb(t.spacing.xs)
        .text_size(t.typography.small)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(text)
}

fn body(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .pt(t.spacing.xl)
        .px(t.spacing.xxl)
        .pb(t.spacing.xxl)
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap(t.spacing.md)
                .children(STATS.map(|entry| stat(t, entry))),
        )
        .child(heading(t, "Largest files"))
        .children(LARGEST.map(|entry| largest(t, entry)))
}

pub fn sheet(t: &Theme, state: &Entity<SheetState>, frame: SheetFrame) -> SideSheet {
    SideSheet::new("preview-sheet", state, frame)
        .label("Preview")
        .header(title(t))
        .close("escape", |_, _| {})
        .body(body(t))
        .hint(&["ctrl-shift-q"], "Browse")
        .hint(&["enter", "f3"], "Open")
        .hint(&["escape"], "Stop scan")
        .hint(&["[", "]"], "")
}
