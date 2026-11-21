//! The View preferences as an app global: installs from the profile, reads them and saves them back.

use std::path::Path;

use gpui_kit::Global;
use gpui_kit::WindowAppearance;
use navo_compare_profile::appearance::{
    AppearanceError, AppearanceMode, PaletteId, UiPrefs, load_or_default, save,
};
use navo_compare_profile::book::profile_text;
use navo_compare_profile::paths::{Profile, open_profile};
use sidewise_kit::prelude::*;
use sidewise_kit::tokens::{Mode, ThemeChoice};

use crate::theme::mode_of;

/// The View preferences and the profile they save to; `profile` is None when no profile opened.
struct Prefs {
    prefs: UiPrefs,
    profile: Option<Profile>,
}
impl Global for Prefs {}

/// Installs the preferences global from the profile (`explicit` folder, else the platform folders).
/// Always installs; on a profile or read error it installs the defaults with no profile and returns
/// `Err("View settings are not saved: <reason>")`.
pub(crate) fn install_prefs(explicit: Option<&Path>, cx: &mut App) -> Result<(), String> {
    match open_profile(explicit) {
        Ok(profile) => match load_or_default(&profile) {
            Ok(prefs) => {
                cx.set_global(Prefs {
                    prefs,
                    profile: Some(profile),
                });
                Ok(())
            }
            Err(error) => {
                cx.set_global(Prefs {
                    prefs: UiPrefs::default(),
                    profile: None,
                });
                Err(format!(
                    "View settings are not saved: {}",
                    appearance_text(&error)
                ))
            }
        },
        Err(error) => {
            cx.set_global(Prefs {
                prefs: UiPrefs::default(),
                profile: None,
            });
            Err(format!(
                "View settings are not saved: {}",
                profile_text(&error)
            ))
        }
    }
}

/// The current preferences, or the defaults when no global is installed.
pub(crate) fn prefs(cx: &App) -> UiPrefs {
    cx.try_global::<Prefs>()
        .map(|stored| stored.prefs.clone())
        .unwrap_or_default()
}

/// Sets the global to `prefs` first, then saves to the profile when one exists.
/// A save error returns `Err("Could not save the View settings: <reason>")` (the global keeps `prefs`).
pub(crate) fn save_prefs(prefs: UiPrefs, cx: &mut App) -> Result<(), String> {
    if cx.try_global::<Prefs>().is_none() {
        cx.set_global(Prefs {
            prefs,
            profile: None,
        });
        return Ok(());
    }
    // `Profile` is not `Clone`, so update the global in place; the profile stays where it was.
    let stored = cx.global_mut::<Prefs>();
    stored.prefs = prefs;
    let Some(profile) = &stored.profile else {
        return Ok(());
    };
    save(profile, &stored.prefs).map_err(|error| {
        format!(
            "Could not save the View settings: {}",
            appearance_text(&error)
        )
    })
}

/// The saved palette and the mode it resolves to under the OS `appearance`: System follows the OS,
/// Light and Dark pin it (NC152). `None` when no preferences global is installed (unit tests).
pub(crate) fn theme_choice(appearance: WindowAppearance, cx: &App) -> Option<(PaletteId, Mode)> {
    let stored = cx.try_global::<Prefs>()?;
    let mode = choice_of(stored.prefs.mode).resolve(mode_of(appearance));
    Some((stored.prefs.palette, mode))
}

/// Maps the saved mode one-to-one onto the kit's theme choice.
fn choice_of(mode: AppearanceMode) -> ThemeChoice {
    match mode {
        AppearanceMode::System => ThemeChoice::System,
        AppearanceMode::Light => ThemeChoice::Light,
        AppearanceMode::Dark => ThemeChoice::Dark,
    }
}

/// Describes an appearance error as the I/O error and the path it happened on.
fn appearance_text(error: &AppearanceError) -> String {
    match error {
        AppearanceError::Io { path, error } => format!("{error} ({})", path.display()),
    }
}
