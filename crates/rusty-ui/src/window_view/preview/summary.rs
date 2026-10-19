use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{MouseButton, Role, ScrollHandle, Stateful, WeakEntity};
use rusty_fs::{EntryKind, FolderProfile, LargeFile, Tally};

use super::contents_view::{Mark, pick};
use super::spot::Spot;
use crate::file_list::kind;
use crate::file_list::row::inset_ring;
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::FerriteWindow;

const HEADING_HEIGHT: f32 = 28.;
const ROW_HEIGHT: f32 = 24.;
const BAR_HEIGHT: f32 = 8.;
const SWATCH: f32 = 8.;
const TYPES_AT: usize = 2;
pub(super) const LARGEST_AT: usize = 4;

pub(super) fn heading(
    t: &Theme,
    id: &'static str,
    label: &str,
    count: String,
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_label(label.to_owned())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .h(px(HEADING_HEIGHT))
        .bg(t.surfaces.sheet)
        .border_b_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(label.to_owned()),
        )
        .child(div().flex_1())
        .child(count)
}

fn swatch_color(t: &Theme, extension: &str) -> Hsla {
    kind::icon(EntryKind::File, extension, &t.colors).1
}

fn type_name(t: &Theme, color: Hsla, name: String) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .w(px(76.))
        .font_family(t.typography.mono_font.clone())
        .child(div().size(px(SWATCH)).rounded(px(2.)).bg(color))
        .child(div().truncate().child(name))
}

fn type_row(t: &Theme, (extension, tally): (&str, Tally), most: u64) -> Div {
    let color = swatch_color(t, extension);
    let share = if most == 0 {
        0.
    } else {
        tally.bytes as f32 / most as f32
    };
    let name = if extension.is_empty() {
        "(none)".to_owned()
    } else {
        format!(".{extension}")
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.lg)
        .h(px(ROW_HEIGHT))
        .text_size(t.typography.small)
        .child(type_name(t, color, name))
        .child(
            div()
                .flex_none()
                .w(px(56.))
                .text_right()
                .text_color(t.colors.muted)
                .child(format::bytes(tally.files)),
        )
        .child(
            div()
                .flex_none()
                .w(px(72.))
                .text_right()
                .child(format::size(tally.bytes)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(40.))
                .h(px(BAR_HEIGHT))
                .rounded(t.radii.full)
                .bg(t.surfaces.field)
                .shadow(inset_ring(t.colors.line))
                .overflow_hidden()
                .child(div().h_full().w(relative(share)).bg(color)),
        )
}

fn types(t: &Theme, profile: Option<&FolderProfile>) -> impl IntoElement + use<> {
    let mut rows: Vec<(&str, Tally)> = profile
        .map(|profile| {
            profile
                .kinds
                .iter()
                .map(|(extension, tally)| (extension.as_str(), *tally))
                .collect()
        })
        .unwrap_or_default();
    if let Some(other) = profile.map(|profile| profile.other).filter(|o| o.files > 0) {
        rows.push(("other", other));
    }
    let most = rows.iter().map(|(_, tally)| tally.bytes).max().unwrap_or(0);
    div()
        .id("preview-types")
        .test_support()
        .flex()
        .flex_col()
        .py(t.spacing.xs)
        .children(rows.into_iter().map(|row| type_row(t, row, most)))
}

fn type_count(profile: Option<&FolderProfile>) -> String {
    let count = profile.map_or(0, |profile| {
        profile.kinds.len() + usize::from(profile.other.files > 0)
    });
    format::count(count as u64, "kind", "kinds")
}

pub(in crate::window_view) struct Picks {
    pub view: WeakEntity<FerriteWindow>,
    pub spot: Option<Spot>,
    pub is_browsing: bool,
    pub marked: Vec<usize>,
}

fn largest_name(t: &Theme, name: &str, is_marked: bool) -> Div {
    div()
        .flex_none()
        .font_weight(FontWeight::SEMIBOLD)
        .when(is_marked, |name| name.text_color(t.colors.mark))
        .child(name.to_owned())
}

fn largest_size(t: &Theme, bytes: u64, is_marked: bool) -> Div {
    div()
        .flex_none()
        .when(is_marked, |size| {
            size.text_color(t.colors.mark)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .child(format::size(bytes))
}

fn largest_row(t: &Theme, (rank, file): (usize, &LargeFile), picks: &Picks) -> impl IntoElement {
    let cut = file.path.rfind(['\\', '/']).map_or(0, |at| at + 1);
    let (folder, name) = file.path.split_at(cut);
    let is_spot = picks.spot == Some(Spot::Largest(rank));
    let is_marked = picks.marked.contains(&rank);
    let row = div()
        .id(("preview-largest", rank))
        .aria_label(file.path.clone())
        .aria_selected(is_spot)
        .test_support()
        .relative()
        .pr(t.spacing.xs)
        .rounded(t.radii.md)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, pick(&picks.view, Spot::Largest(rank)))
        .flex()
        .items_center()
        .gap(t.spacing.lg)
        .h(px(ROW_HEIGHT))
        .text_size(t.typography.small)
        .child(
            div()
                .flex_none()
                .w(px(18.))
                .text_right()
                .text_color(t.colors.muted)
                .child((rank + 1).to_string()),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_shrink(1.)
                        .min_w_0()
                        .truncate()
                        .text_color(t.colors.muted)
                        .child(folder.to_owned()),
                )
                .child(largest_name(t, name, is_marked)),
        )
        .child(largest_size(t, file.bytes, is_marked));
    Mark::of(is_spot, picks.is_browsing).paint(row, t)
}

fn largest(t: &Theme, profile: Option<&FolderProfile>, picks: &Picks) -> impl IntoElement + use<> {
    let files = profile
        .map(|profile| profile.largest.as_slice())
        .unwrap_or_default();
    div().flex().flex_col().py(t.spacing.xs).children(
        files
            .iter()
            .enumerate()
            .map(|row| largest_row(t, row, picks)),
    )
}

pub(in crate::window_view) struct Sections {
    pub stats: Div,
    pub profile: Option<FolderProfile>,
    pub scroll: ScrollHandle,
    pub picks: Picks,
}

pub(super) fn summary(t: &Theme, sections: Sections) -> impl IntoElement + use<> {
    let profile = sections.profile.as_ref();
    let types_heading = |id| heading(t, id, "By type", type_count(profile));
    let largest_heading = |id| heading(t, id, "Largest files", "top 10".to_owned());
    let pinned = match sections.scroll.top_item() {
        TYPES_AT => Some(types_heading("preview-pinned-heading")),
        top if top >= LARGEST_AT => Some(largest_heading("preview-pinned-heading")),
        _ => None,
    };
    let column = div()
        .id("preview-summary")
        .test_support()
        .size_full()
        .overflow_y_scroll()
        .track_scroll(&sections.scroll)
        .flex()
        .flex_col()
        .px(t.spacing.xxl)
        .pb(t.spacing.xxl)
        .child(sections.stats.pt(t.spacing.xl).pb(t.spacing.md))
        .child(types_heading("preview-types-heading"))
        .child(types(t, profile))
        .child(largest_heading("preview-largest-heading"))
        .child(largest(t, profile, &sections.picks));
    div()
        .relative()
        .size_full()
        .min_h_0()
        .child(column)
        .children(pinned.map(|heading| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .px(t.spacing.xxl)
                .bg(t.surfaces.sheet)
                .child(heading)
        }))
}
