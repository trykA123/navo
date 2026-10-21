use std::ops::Range;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Div, FontWeight, HighlightStyle, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    StyledText, div, px,
};
use rusty_fs::EntryKind;
use rusty_l10n::{t, tr};

use super::super::results::{Heading, Row};
use super::super::source::{Group, Hit, Target};
use crate::file_list::kind;
use crate::primitives::{IconName, Kbd, fit_name, icon};
use crate::tokens::Theme;

pub(super) const ROW: f32 = 32.;
const HEADER: f32 = 24.;
const TOP_HIT: f32 = 52.;
const TILE: f32 = 32.;
const BADGE: f32 = 18.;
const ELLIPSIS: char = '…';
const PREFIXES: [(&str, &str); 3] = [
    (">", "command-center-prefix-commands"),
    ("/", "command-center-prefix-files"),
    ("C:\\", "command-center-prefix-paths"),
];

pub(super) type RowElement = ObservedElement<Stateful<Div>>;

pub(super) struct RowLook<'a> {
    pub theme: &'a Theme,
    pub detail_chars: usize,
    pub gutter: Pixels,
}

pub(crate) fn highlight_ranges(text: &str, indices: &[u32]) -> Vec<Range<usize>> {
    let mut wanted: Vec<usize> = indices.iter().map(|index| *index as usize).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut wanted = wanted.into_iter().peekable();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (position, (byte, letter)) in text.char_indices().enumerate() {
        if wanted.next_if_eq(&position).is_none() {
            continue;
        }
        let end = byte + letter.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == byte => last.end = end,
            _ => ranges.push(byte..end),
        }
    }
    ranges
}

pub(super) fn render(look: &RowLook, row: &Row, index: usize, is_selected: bool) -> RowElement {
    let t = look.theme;
    match row {
        Row::Header(heading) => header(t, heading_label(heading), index),
        Row::TopHit(hit) => top_hit(look, hit, index, is_selected),
        Row::Hit(hit) => hit_row(look, hit, index, is_selected),
        Row::ShowAll { group, total } => show_all(look, (*group, *total), index, is_selected),
        Row::Hint => prefix_hint(t, index),
    }
}

fn heading_label(heading: &Heading) -> SharedString {
    match heading {
        Heading::TopHit => tr("command-center-top-hit").into(),
        Heading::Recent => tr("command-center-recent").into(),
        Heading::Group(Group::Commands) => tr("command-center-commands").into(),
        Heading::Group(Group::Folders) => tr("command-center-folders").into(),
        Heading::Group(Group::Files) => tr("command-center-files").into(),
        Heading::Group(Group::Module(name)) => (*name).into(),
        Heading::FilesCapped { cap, root } => t!(
            "command-center-files-capped",
            count = crate::format::bytes(*cap as u64),
            root = root.to_string()
        )
        .into(),
        Heading::Offline(path) => t!("command-center-path-offline", path = path.to_string()).into(),
    }
}

fn base(index: usize) -> RowElement {
    div()
        .id(("command-center-row", index))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .w_full()
}

fn option(look: &RowLook, index: usize, is_selected: bool, height: f32) -> RowElement {
    let t = look.theme;
    base(index)
        .role(Role::ListBoxOption)
        .aria_selected(is_selected)
        .gap(t.spacing.md)
        .h(px(height))
        .pl(t.spacing.md)
        .pr(t.spacing.md + look.gutter)
        .rounded(t.radii.card)
        .cursor_pointer()
        .when(is_selected, |row| row.bg(t.colors.selection))
}

fn header(t: &Theme, label: SharedString, index: usize) -> RowElement {
    base(index)
        .role(Role::Heading)
        .aria_label(label.clone())
        .items_end()
        .h(px(HEADER))
        .px(t.spacing.md)
        .pb(t.spacing.xxs)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(div().min_w_0().truncate().child(label))
}

fn shown_title(hit: &Hit) -> SharedString {
    let is_arg = matches!(hit.target, Target::Arg(..));
    if is_arg && !hit.title.ends_with(ELLIPSIS) {
        return format!("{}{ELLIPSIS}", hit.title).into();
    }
    hit.title.clone()
}

fn title(t: &Theme, hit: &Hit, text: SharedString) -> Div {
    let found = HighlightStyle {
        color: Some(t.colors.accent_text),
        font_weight: Some(FontWeight::MEDIUM),
        ..HighlightStyle::default()
    };
    let runs: Vec<_> = highlight_ranges(&text, &hit.indices)
        .into_iter()
        .map(|range| (range, found))
        .collect();
    div()
        .flex_shrink(1.)
        .min_w_0()
        .truncate()
        .child(StyledText::new(text).with_highlights(runs))
}

fn hit_icon(t: &Theme, hit: &Hit) -> (IconName, Hsla) {
    match &hit.target {
        Target::Action(_) | Target::Core(_) | Target::Arg(..) => {
            (IconName::SquareChevronRight, t.colors.accent_text)
        }
        Target::Open { folder: true, .. } | Target::GoTo(_) => (IconName::Folder, t.colors.folder),
        Target::Open { folder: false, .. } => {
            let (_, extension) = kind::split_name(&hit.title, EntryKind::File);
            kind::icon(EntryKind::File, extension, &t.colors)
        }
        Target::ShowAll(_) => (IconName::ChevronRight, t.colors.muted),
    }
}

fn is_command(hit: &Hit) -> bool {
    matches!(
        hit.target,
        Target::Action(_) | Target::Core(_) | Target::Arg(..)
    )
}

fn detail(t: &Theme, text: &str, chars: usize) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(fit_name("", text, chars).rest)
}

fn shortcut(t: &Theme, keys: Option<&SharedString>) -> Option<Div> {
    let keys = keys?;
    let caps = keys
        .split_whitespace()
        .enumerate()
        .map(|(index, part)| -> AnyElement {
            match Kbd::new(("command-center-key", index), part) {
                Some(key) => key.into_any_element(),
                None => div().child(part.to_owned()).into_any_element(),
            }
        });
    Some(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .ml_auto()
            .children(caps),
    )
}

fn hit_row(look: &RowLook, hit: &Hit, index: usize, is_selected: bool) -> RowElement {
    let t = look.theme;
    let text = shown_title(hit);
    let (name, color) = hit_icon(t, hit);
    option(look, index, is_selected, ROW)
        .aria_label(text.clone())
        .child(icon(name, t.spacing.icon, color))
        .child(title(t, hit, text))
        .child(detail(t, &hit.detail, look.detail_chars))
        .children(shortcut(t, hit.shortcut.as_ref()))
}

fn badge(t: &Theme, hit: &Hit) -> Option<Div> {
    let text = match hit.target {
        Target::Open { folder: true, .. } | Target::GoTo(_) => tr("command-center-folder"),
        Target::Open { folder: false, .. } => {
            let (_, extension) = kind::split_name(&hit.title, EntryKind::File);
            extension.to_uppercase()
        }
        _ => return None,
    };
    (!text.is_empty()).then(|| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .ml_auto()
            .h(px(BADGE))
            .px(t.spacing.sm)
            .rounded(t.radii.sm)
            .bg(t.colors.rail)
            .text_size(t.typography.tiny)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.muted)
            .child(text)
    })
}

fn top_hit(look: &RowLook, hit: &Hit, index: usize, is_selected: bool) -> RowElement {
    let t = look.theme;
    let text = shown_title(hit);
    let (name, color) = hit_icon(t, hit);
    let line = if is_command(hit) {
        t!(
            "command-center-command-detail",
            context = hit.detail.to_string()
        )
    } else {
        hit.detail.to_string()
    };
    let tile = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(TILE))
        .rounded(t.radii.card)
        .border_1()
        .border_color(t.surfaces.tile_edge)
        .bg(t.surfaces.tile)
        .child(icon(name, t.spacing.icon, color));
    let lines = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(title(t, hit, text.clone()).font_weight(FontWeight::SEMIBOLD))
        .child(detail(t, &line, look.detail_chars));
    let right = shortcut(t, hit.shortcut.as_ref()).or_else(|| badge(t, hit));
    option(look, index, is_selected, TOP_HIT)
        .aria_label(text)
        .gap(t.spacing.xl)
        .child(tile)
        .child(lines)
        .children(right)
}

fn show_all(
    look: &RowLook,
    (group, total): (Group, usize),
    index: usize,
    is_selected: bool,
) -> RowElement {
    let t = look.theme;
    let label: SharedString = t!("command-center-show-all", count = total).into();
    let what = match group {
        Group::Commands => tr("command-center-show-all-commands"),
        Group::Folders | Group::Files => tr("command-center-show-all-files"),
        Group::Module(name) => name.to_owned(),
    };
    option(look, index, is_selected, ROW)
        .aria_label(label.clone())
        .child(icon(IconName::ChevronRight, t.spacing.icon, t.colors.muted))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.accent_text)
                .child(label),
        )
        .child(detail(t, &what, look.detail_chars))
}

fn prefix_hint(t: &Theme, index: usize) -> RowElement {
    let label = PREFIXES
        .iter()
        .map(|(prefix, id)| format!("{prefix} {}", tr(id)))
        .collect::<Vec<_>>()
        .join(" · ");
    base(index)
        .role(Role::Note)
        .aria_label(label)
        .gap(t.spacing.xl)
        .h(px(ROW))
        .px(t.spacing.md)
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .children(PREFIXES.iter().map(|(prefix, id)| {
            div()
                .flex()
                .gap(t.spacing.xs)
                .child(
                    div()
                        .font_family(t.typography.mono_font.clone())
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.accent_text)
                        .child(*prefix),
                )
                .child(tr(id))
        }))
}
