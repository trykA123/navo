//! The language picker model: the menu rows and the mapping back from a chosen row id.
//! It mirrors `encoding.rs` and feeds the picker menu opened from the viewer.

mod menu;

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use navo_core::shell_menu::{Group, ItemId, MenuItem, ShellMenu};
use navo_syntax::Registry;
use sidewise_kit::languages::known_languages;

use super::ChooseLanguage;
use super::syntax::LanguageChoice;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};

const AUTO: &str = "language.auto";
const PLAIN: &str = "language.plain";
const LOG_ID: &str = "language.log";
const LOG: &str = "log";
/// Starts the id of every kit or rule language row, so no name can equal a fixed row's id.
const NAMED: &str = "language:";

/// The header button that opens the language picker; clicking it dispatches `ChooseLanguage`.
pub fn language_button() -> impl IntoElement {
    Button::new("viewer-language", navo_l10n::tr("viewer-language-button"))
        .variant(ButtonVariant::Ghost)
        .tooltip(navo_l10n::tr("viewer-language-tooltip"))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ChooseLanguage), cx))
}

/// Window width below which an open Find or Go to bar leaves no room for the Syntax button.
/// The bar is ~450 px wide and the rest of the header ~500 px, so Find and View stay on screen.
const BAR_NEEDS: f32 = 1000.;

/// The Syntax button when `is_text` shows it, hidden while a search bar crowds a narrow window.
/// The picker stays on Ctrl+Shift+L and the View menu.
pub fn language_control(
    is_text: bool,
    is_bar_open: bool,
    window: &Window,
) -> Option<impl IntoElement> {
    let fits = bar_fits(is_bar_open, window.viewport_size().width);
    (is_text && fits).then(language_button)
}

/// Whether the header has room for the Syntax button beside a search bar in a window `width` wide.
fn bar_fits(is_bar_open: bool, width: Pixels) -> bool {
    !is_bar_open || width >= px(BAR_NEEDS)
}

/// Builds one picker row; `is_on` puts the check mark on it.
fn row(id: &'static str, text: &str, is_on: bool) -> MenuItem {
    MenuItem {
        checked: is_on,
        ..MenuItem::new(ItemId::Action(id), text, Group::Navo)
    }
}

/// Fluent arguments for the Auto row: `name` is the language Auto detects.
fn auto_args(auto: &str) -> navo_l10n::FluentArgs<'static> {
    let mut args = navo_l10n::FluentArgs::new();
    args.set("name", auto.to_owned());
    args
}

/// Builds the picker rows for the viewer's `current` choice; `auto` names what Auto detects and
/// `registry` supplies the rule languages, so a test can pass its own instead of the global one.
pub(super) fn language_picker(
    current: &LanguageChoice,
    auto: &str,
    registry: &Registry,
) -> ShellMenu {
    let named = |id: &str| matches!(current, LanguageChoice::Named(name) if name == id);
    let mut items = vec![
        row(
            AUTO,
            &navo_l10n::tr_args("viewer-language-auto", &auto_args(auto)),
            *current == LanguageChoice::Auto,
        ),
        row(
            PLAIN,
            &navo_l10n::tr("viewer-language-plain"),
            *current == LanguageChoice::Plain,
        ),
        row(LOG_ID, &navo_l10n::tr("viewer-language-log"), named(LOG)),
        MenuItem::separator(Group::Navo),
    ];
    items.extend(
        names(registry)
            .into_iter()
            .map(|name| row(named_id(name), name, named(name))),
    );
    ShellMenu { items }
}

/// The sorted, unique kit grammar and rule language names, without the fixed `log` row.
fn names(registry: &Registry) -> Vec<&str> {
    let mut names = known_languages();
    names.extend(registry.iter().map(|language| language.name.as_str()));
    names.retain(|name| *name != LOG);
    names.sort_unstable();
    names.dedup();
    names
}

/// The row id of a language `name`; each name is leaked once, so a menu rebuild costs nothing.
fn named_id(name: &str) -> &'static str {
    static IDS: Mutex<BTreeMap<String, &'static str>> = Mutex::new(BTreeMap::new());
    let mut ids = IDS.lock().unwrap_or_else(PoisonError::into_inner);
    ids.entry(name.to_owned())
        .or_insert_with(|| Box::leak(format!("{NAMED}{name}").into_boxed_str()))
}

/// Maps a picked row id back to the choice it stands for; an id this menu never made is Auto.
pub(super) fn choice_of(id: &str) -> LanguageChoice {
    match id {
        PLAIN => LanguageChoice::Plain,
        LOG_ID => LanguageChoice::Named(LOG.to_owned()),
        _ => id.strip_prefix(NAMED).map_or(LanguageChoice::Auto, |name| {
            LanguageChoice::Named(name.to_owned())
        }),
    }
}

#[cfg(test)]
mod tests;
