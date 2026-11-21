//! The compare host: a compare screen over the default diff backend and the compare keys.
//! The compare tab mounts these (H2).

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{App, KeyBinding, KeyBindingContextPredicate};
use navo_compare_profile::appearance::{LayoutPref, UiPrefs, ViewPresetPref};
use sidewise_host::compare::diff::copy_port::CopyHost;
use sidewise_host::compare::diff::hooks::{BackendParts, DiffBackend};
use sidewise_host::compare::diff::write::host::LocalCopyHost;
use sidewise_host::compare::dirs::CompareDirs;
use sidewise_ui::browser::Waker;
use sidewise_ui::compare_diff::{CompareOptions, DiffHooks, Layout};
use sidewise_ui::compare_keys::compare_keys;
use sidewise_ui::list_actions::UndoLast;
use sidewise_ui::window_view::{CompareScreen, DiffSlot, LookDefaults, LookPreset};

use super::git_sides::{shared_sides, side_fn};
use super::host_parts::compare_files;

#[cfg(test)]
thread_local! {
    /// Test seam: diff hooks that replace the in-memory backend on this thread.
    pub(crate) static TEST_DIFF_HOOKS: std::cell::RefCell<Option<Arc<dyn DiffHooks>>> =
        const { std::cell::RefCell::new(None) };
}

/// The diff hooks a new screen uses: the in-memory backend with the journal's copy host when
/// it opened (else copies are refused), or the test seam's hooks.
pub(crate) fn diff_hooks() -> Arc<dyn DiffHooks> {
    #[cfg(test)]
    if let Some(hooks) = TEST_DIFF_HOOKS.with_borrow(Clone::clone) {
        return hooks;
    }
    diff_hooks_for(crate::writes::host())
}

/// The diff hooks over the Git-aware backend, writing copies through `host` when there is one.
pub(crate) fn diff_hooks_for(host: Option<Arc<LocalCopyHost>>) -> Arc<dyn DiffHooks> {
    Arc::new(hooks_with(host.map(|host| host as Arc<dyn CopyHost>)))
}

/// The diff backend over local and Git sides and the process-wide Viewed store, writing copies
/// through `host` when there is one.
pub(crate) fn hooks_with(host: Option<Arc<dyn CopyHost>>) -> DiffBackend {
    let backend = DiffBackend::new(BackendParts {
        dirs: CompareDirs::default(),
        side: side_fn(shared_sides()),
        viewed: Arc::new(crate::viewed_store::shared),
    });
    match host {
        Some(host) => backend.with_copy_host(host),
        None => backend,
    }
}

/// The screen's look defaults for the saved View preferences: preset, words, wrap and layout
/// (Split follows the window, Unified is forced).
pub fn look_of(p: &UiPrefs) -> LookDefaults {
    LookDefaults {
        preset: match p.preset {
            ViewPresetPref::Simple => LookPreset::Simple,
            ViewPresetPref::Rich => LookPreset::Rich,
            ViewPresetPref::Custom => LookPreset::Custom,
        },
        words: p.words,
        wrap: p.wrap_lines,
        layout: match p.layout {
            LayoutPref::Split => None,
            LayoutPref::Unified => Some(Layout::Unified),
        },
    }
}

/// A compare screen over the in-memory diff backend (NC118) that opens pairs with `look`;
/// `wake` runs after each diff update.
pub fn new_compare_screen(wake: Waker, look: LookDefaults) -> CompareScreen {
    let hooks = diff_hooks();
    let mut screen = CompareScreen::new(
        DiffSlot::new(Some(hooks), wake),
        CompareOptions::default(),
        compare_files(),
    );
    screen.set_look(look);
    screen
}

/// Binds Navo's compare keys (contexts `CompareView`, `CompareStream`) and Ctrl+Z to undo the
/// last copy there; returns how many bindings it added. A bind whose action or keystroke does
/// not load is skipped.
pub fn bind_compare_keys(cx: &mut App) -> usize {
    let Ok(keys) = compare_keys() else {
        return 0;
    };
    let mut all = Vec::new();
    for entry in keys {
        let Some(name) = action_name(&entry.command) else {
            continue;
        };
        let Ok(action) = cx.build_action(&name, None) else {
            continue;
        };
        let predicate = KeyBindingContextPredicate::parse(&entry.context)
            .ok()
            .map(Rc::new);
        for key in &entry.keys {
            let action = action.boxed_clone();
            if let Ok(binding) = KeyBinding::load(key, action, predicate.clone(), false, None, cx) {
                all.push(binding);
            }
        }
    }
    // The key file's `files.undo` names no action; Ctrl+Z undoes the last copy (NC148).
    for context in ["CompareView", "CompareStream"] {
        all.push(KeyBinding::new("ctrl-z", UndoLast, Some(context)));
    }
    let count = all.len();
    cx.bind_keys(all);
    count
}

/// The action type name of a compare command (`compare.next_difference` becomes
/// `compare::NextDifference`); `None` when the command has no namespace dot.
pub(crate) fn action_name(command: &str) -> Option<String> {
    let (namespace, rest) = command.split_once('.')?;
    let action = rest
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            let first = chars.next()?;
            Some(first.to_uppercase().collect::<String>() + chars.as_str())
        })
        .collect::<Option<Vec<_>>>()?
        .join("");
    Some(format!("{namespace}::{action}"))
}
