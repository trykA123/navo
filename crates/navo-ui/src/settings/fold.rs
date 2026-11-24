//! Advanced folds in Settings: which settings are advanced, the fold row at the end of each
//! group that holds them, which folds are open, and the walk order of a page's rows
//! (README "Advanced settings").

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::SheetEvent;
use super::fields::{FIELDS, Field, Section};
use super::find::Filter;
use super::pages::{GROUPS, Group};
use super::row_state::GUTTER;
use super::steps::HELP_ROW;
use super::view::{Look, row_frame, send_on_click};
use crate::prelude::*;
use crate::primitives::{IconName, icon};

/// The row id of the first group's fold row; fold rows sit above every setting and the Help row.
const FIRST_FOLD: usize = HELP_ROW + 1;

/// The rare settings that sit in the fold of their group (README map, "Advanced fold").
const ADVANCED: [Field; 14] = [
    Field::WindowExtWidth,
    Field::WindowSizeWidth,
    Field::WindowDateWidth,
    Field::RenameLog,
    Field::CopyMode,
    Field::RetentionCapMib,
    Field::RetentionMinFreeMib,
    Field::SearchDrives,
    Field::SearchExclude,
    Field::AutomotiveModules,
    Field::GitEngine,
    Field::GitNetwork,
    Field::CompareGitEngine,
    Field::CompareBudgetMib,
];

const _: () = assert!(GROUPS.len() <= u32::BITS as usize, "one fold bit per group");

impl Field {
    /// True for a rare setting that sits in the fold at the end of its group.
    pub fn is_advanced(self) -> bool {
        ADVANCED.contains(&self)
    }
}

/// Which folds are open, one bit per group in `GROUPS`; all closed by default. The Settings
/// window keeps it while the dialog is open; it never reaches the settings file.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Folds(u32);

impl Folds {
    /// True when `row` is a fold row and its fold is open.
    pub fn is_open(self, row: usize) -> bool {
        group_of_fold(row).is_some_and(|at| self.0 & (1 << at) != 0)
    }

    /// Opens or closes the fold on fold row `row`; any other row changes nothing.
    pub fn set(&mut self, row: usize, open: bool) {
        let Some(at) = group_of_fold(row) else {
            return;
        };
        if open {
            self.0 |= 1 << at;
        } else {
            self.0 &= !(1 << at);
        }
    }

    /// Opens the fold that holds setting row `row`, so a picked advanced setting is shown.
    pub fn reveal(&mut self, row: usize) {
        if let Some(fold) = FIELDS.get(row).and_then(|spec| fold_row(spec.field)) {
            self.set(fold, true);
        }
    }

    /// False for an advanced setting whose fold is closed.
    pub(super) fn shows(self, field: Field) -> bool {
        fold_row(field).is_none_or(|fold| self.is_open(fold))
    }
}

/// The fold row that holds `field`, or `None` when the field is not advanced.
pub fn fold_row(field: Field) -> Option<usize> {
    if !field.is_advanced() {
        return None;
    }
    let at = GROUPS
        .iter()
        .position(|group| group.fields.contains(&field))?;
    Some(FIRST_FOLD + at)
}

/// The fold row of an advanced setting row, or `row` itself; used when the folds close.
pub fn outer_row(row: usize) -> usize {
    FIELDS
        .get(row)
        .and_then(|spec| fold_row(spec.field))
        .unwrap_or(row)
}

/// `row` when it is a fold row, else `row` held at most at the Help row.
pub fn clamped_row(row: usize) -> usize {
    if group_of_fold(row).is_some() {
        row
    } else {
        row.min(HELP_ROW)
    }
}

/// The index in `GROUPS` of fold row `row`, or `None` for any other row.
fn group_of_fold(row: usize) -> Option<usize> {
    row.checked_sub(FIRST_FOLD).filter(|at| *at < GROUPS.len())
}

fn page_of(group: &Group) -> Option<Section> {
    group.fields.first().map(|field| field.spec().section)
}

/// The page of fold row `row`, or `None` for any other row.
pub(super) fn fold_section(row: usize) -> Option<Section> {
    GROUPS.get(group_of_fold(row)?).and_then(page_of)
}

/// The rows of `page` in walk order: each group's shown settings, then (Find empty) its fold
/// row, then its advanced settings that the fold or Find shows.
pub(super) fn page_rows(filter: &Filter, page: Section) -> Vec<usize> {
    GROUPS
        .iter()
        .enumerate()
        .filter(|(_, group)| page_of(group) == Some(page))
        .flat_map(|(at, group)| group_rows(filter, at, group))
        .collect()
}

fn group_rows(filter: &Filter, at: usize, group: &Group) -> Vec<usize> {
    let shown = |advanced: bool| {
        group
            .fields
            .iter()
            .filter(move |field| field.is_advanced() == advanced && filter.admits(field.spec()))
            .map(|field| field.index())
    };
    let fold = (filter.is_empty() && advanced_count(filter, group) > 0).then_some(FIRST_FOLD + at);
    shown(false).chain(fold).chain(shown(true)).collect()
}

/// How many advanced settings of `group` would show with the fold open.
fn advanced_count(filter: &Filter, group: &Group) -> usize {
    group
        .fields
        .iter()
        .filter(|field| field.is_advanced() && filter.matches(field.spec()))
        .count()
}

/// Left: closes the fold from its row or from a setting inside it and selects the fold row;
/// `None` during Find or elsewhere, so Left changes the value as usual.
pub(super) fn left_event(filter: &Filter, row: usize) -> Option<SheetEvent> {
    let fold = group_of_fold(row).map_or_else(|| fold_of(row), |_| Some(row))?;
    filter.is_empty().then_some(SheetEvent::Fold(fold, false))
}

fn fold_of(row: usize) -> Option<usize> {
    FIELDS.get(row).and_then(|spec| fold_row(spec.field))
}

/// Right: opens the fold from its row; `None` on any other row.
pub(super) fn right_event(filter: &Filter, row: usize) -> Option<SheetEvent> {
    (filter.is_empty() && group_of_fold(row).is_some()).then_some(SheetEvent::Fold(row, true))
}

/// The fold row: a chevron and "N advanced settings" in the setting row style; a click opens
/// or closes the fold and selects the row.
pub(super) fn fold_line(look: &Look, fold: usize) -> AnyElement {
    let t = look.theme;
    let is_open = look.filter.folds().is_open(fold);
    let is_current = fold == look.row;
    let count = group_of_fold(fold)
        .and_then(|at| GROUPS.get(at))
        .map_or(0, |group| advanced_count(look.filter, group));
    let text: SharedString = match count {
        1 => "1 advanced setting".into(),
        _ => format!("{count} advanced settings").into(),
    };
    let glyph = if is_open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let content = t.selected_content(is_current);
    let chevron = div()
        .flex()
        .flex_none()
        .justify_center()
        .w(px(GUTTER))
        .child(icon(glyph, t.spacing.icon, content.colors.muted));
    let line = row_frame(t, is_current)
        .id(("settings-fold", fold))
        .role(Role::ListItem)
        .aria_label(text.clone())
        .aria_expanded(is_open)
        .aria_selected(is_current)
        .test_support()
        .flex()
        .items_center()
        .gap_x(t.spacing.xl)
        .child(chevron)
        .child(div().truncate().child(text));
    send_on_click(line, look.send.as_ref(), SheetEvent::Fold(fold, !is_open)).into_any_element()
}
