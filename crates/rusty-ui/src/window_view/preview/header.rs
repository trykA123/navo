use gpui_kit::base::TestSupportExt;
use rusty_fs::EntryKind;

use super::Shown;
use super::follow::Target;
use crate::file_list::kind;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const ICON_BOX: f32 = 32.;

pub(in crate::window_view) struct Title {
    pub glyph: (IconName, Hsla),
    pub name: SharedString,
    pub meta: SharedString,
    pub chip: SharedString,
}

fn file_meta(target: &Target) -> String {
    let size = format::size(target.entry.size);
    match target.entry.modified_ms.map(format::date) {
        Some(date) => format!("{size} · {date}"),
        None => size,
    }
}

fn glyph_of(t: &Theme, target: &Target) -> (IconName, Hsla) {
    let name = target.name();
    let (_, extension) = kind::split_name(&name, target.entry.kind);
    let entry_kind = if target.is_folder() {
        EntryKind::Folder
    } else {
        target.entry.kind
    };
    kind::icon(entry_kind, extension, &t.colors)
}

pub(super) fn title_of(t: &Theme, shown: Option<&Shown>) -> Title {
    let (target, chip): (&Target, SharedString) = match shown {
        Some(Shown::File(file)) => {
            let chip = file
                .kind
                .as_ref()
                .map_or_else(|| "File".into(), |kind| kind.name().into());
            (&file.target, chip)
        }
        Some(Shown::Folder(folder)) => (&folder.target, "Folder".into()),
        Some(Shown::Empty) | None => {
            return Title {
                glyph: (IconName::PanelRight, t.colors.muted),
                name: "Preview".into(),
                meta: "Nothing under the cursor".into(),
                chip: SharedString::default(),
            };
        }
    };
    let meta = if target.is_folder() {
        target.location.as_str().to_owned()
    } else {
        file_meta(target)
    };
    Title {
        glyph: glyph_of(t, target),
        name: target.name().into(),
        meta: meta.into(),
        chip,
    }
}

fn mark(t: &Theme, (glyph, color): (IconName, Hsla)) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(ICON_BOX))
        .rounded(t.radii.card)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .child(icon(glyph, t.spacing.icon, color))
}

fn chip(t: &Theme, text: SharedString) -> impl IntoElement {
    div()
        .id("preview-kind")
        .aria_label(text.clone())
        .test_support()
        .flex_none()
        .px(t.spacing.md)
        .rounded(t.radii.pill)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

pub(super) fn title_view(t: &Theme, title: Title) -> Div {
    let text = div()
        .flex()
        .flex_col()
        .flex_shrink(1.)
        .min_w_0()
        .child(
            div()
                .id("preview-name")
                .aria_label(title.name.clone())
                .test_support()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.name),
        )
        .child(
            div()
                .id("preview-meta")
                .aria_label(title.meta.clone())
                .test_support()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(title.meta),
        );
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(t.spacing.lg)
        .child(mark(t, title.glyph))
        .child(text)
        .when(!title.chip.is_empty(), |row| row.child(chip(t, title.chip)))
        .child(div().flex_1())
}
