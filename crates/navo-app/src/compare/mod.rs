//! Compare backend for the app: sources, hash cache, settings and the session thread.
//!
//! `pane::PaneCompare` is the window's `CompareHooks`: it reads the settings from the UI values
//! and forwards events on its own thread. Its steps, for other screens:
//!
//! 1. Fill `settings::CompareSettings` from the stored `[compare]` settings with
//!    `CompareSettings::from(&settings.compare)`.
//! 2. Build a `handle::CompareStart` (two `CompareTarget`s, the settings, a
//!    `hash_cache::StoreHashCache`) and call `handle::CompareHandle::start` with a
//!    `Sender<CompareEvent>`.
//! 3. Drain the receiver continuously (engine rule 1). Per generation, `Rows` and `Totals` arrive
//!    in order and `Finished` follows the last batch. `Failed` means the compare never started.
//! 4. Call `focus`, `stop_row`, `stop_all`, `refresh` and `cancel` on the handle; none blocks.
//!    Dropping the handle cancels and joins on a helper thread.
//! 5. Show the `Choice::note` of `settings::choose_engine` next to the Git engine setting.
//!
//! `ReadHint::Map` maps nothing: S4 found under 3 % gain, so every side reads buffered.
//! `git_side::GitSide` reads with the engine that `settings::engine_for` chose for the repository.
//! Ignore rules read the root `.gitignore` and `.git/info/exclude` of each side, not nested files.

#![cfg_attr(
    not(test),
    expect(dead_code, reason = "Git sides (U6) and focus (U5) wire the rest")
)]

mod convert;
mod entry;
pub(crate) mod git_side;
pub(crate) mod handle;
pub(crate) mod hash_cache;
pub(crate) mod local;
mod pane;
mod rules;
pub(crate) mod settings;

pub(crate) use pane::PaneCompare;

#[cfg(test)]
mod tests;
