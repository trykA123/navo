//! Owned floating find-results window host: lifecycle, placement and book ownership.

pub(crate) mod keys;
mod open;
mod title;
mod view;

use navo_core::FindMemory;

use crate::find_results::book::FindBook;
use crate::prelude::*;
use crate::window_view::hand_off::{RawWindowHandle, WindowShown};
use crate::window_view::search_hooks::FindHooks;

#[cfg(test)]
pub(super) use open::test_raw_handle;
pub(super) use view::FindWindow;

/// Owns the per-window find book and tracks the floating results window handle and hooks.
pub(super) struct FindHost {
    book: Entity<FindBook>,
    window: Option<(AnyWindowHandle, Entity<FindWindow>)>,
    raw: Option<RawWindowHandle>,
    hidden: bool,
    op: Option<u64>,
    hooks: FindHooks,
}

impl FindHost {
    /// Creates a new find host, restoring saved queries as idle tabs.
    pub(super) fn new(hooks: &FindHooks, memory: &FindMemory, cx: &mut App) -> Self {
        let book = cx.new(|_| {
            let mut book = FindBook::new();
            book.restore(memory.queries().to_vec());
            book
        });
        Self {
            book,
            window: None,
            raw: None,
            hidden: false,
            op: None,
            hooks: hooks.clone(),
        }
    }

    /// Returns the shared find-results book entity owned by this window.
    pub(super) fn book(&self) -> &Entity<FindBook> {
        &self.book
    }

    /// Returns the live floating window handle, if the results window is currently open.
    pub(super) fn window_handle(&self) -> Option<AnyWindowHandle> {
        self.window.as_ref().map(|(handle, _)| *handle)
    }

    /// Returns the live results window view entity, if currently open.
    pub(super) fn view(&self) -> Option<&Entity<FindWindow>> {
        self.window.as_ref().map(|(_, view)| view)
    }

    /// Returns whether a floating results window is currently tracked as open.
    pub(super) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    /// Returns the native raw handle captured when the results window opened.
    pub(super) fn raw(&self) -> Option<RawWindowHandle> {
        self.raw
    }

    /// Returns whether the user hid the results window instead of closing it.
    pub(super) fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Updates whether the results window is hidden.
    pub(super) fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
    }

    /// Shows or hides the floating window with the owner, unless the user hid it.
    pub(super) fn follow_shown(&self, hook: &WindowShown, shown: bool) {
        if self.is_open() && !self.is_hidden() {
            hook(self.raw(), shown);
        }
    }

    /// Returns the pending file-operation id owned by the results window, if any.
    pub(super) fn op(&self) -> Option<u64> {
        self.op
    }

    /// Records or clears the pending file-operation id owned by the results window.
    pub(super) fn set_op(&mut self, op: Option<u64>) {
        self.op = op;
    }

    /// Returns the platform and persistence hooks for the find results window.
    pub(super) fn hooks(&self) -> &FindHooks {
        &self.hooks
    }

    /// Records the freshly opened floating window and marks it shown.
    pub(super) fn set_window(&mut self, handle: AnyWindowHandle, view: Entity<FindWindow>) {
        self.window = Some((handle, view));
        self.hidden = false;
    }

    /// Records the native raw handle captured when the floating window opened.
    pub(super) fn set_raw(&mut self, raw: Option<RawWindowHandle>) {
        self.raw = raw;
    }

    /// Clears tracked window state after the floating window closes or disappears.
    pub(super) fn clear_window(&mut self) {
        self.window = None;
        self.raw = None;
        self.hidden = false;
        self.op = None;
    }
}

// Pins future-atom host accessors without adding lint exceptions.
const _: fn(&FindHost) -> &Entity<FindBook> = FindHost::book;
const _: fn(&FindHost) -> Option<AnyWindowHandle> = FindHost::window_handle;
const _: fn(&FindHost) -> Option<&Entity<FindWindow>> = FindHost::view;
const _: fn(&FindHost) -> bool = FindHost::is_open;
const _: fn(&FindHost) -> Option<RawWindowHandle> = FindHost::raw;
const _: fn(&FindHost) -> bool = FindHost::is_hidden;
const _: fn(&mut FindHost, bool) = FindHost::set_hidden;
const _: fn(&FindHost) -> Option<u64> = FindHost::op;
const _: fn(&mut FindHost, Option<u64>) = FindHost::set_op;
const _: fn(&FindHost) -> &FindHooks = FindHost::hooks;
const _: fn(&mut FindHost) = FindHost::clear_window;
