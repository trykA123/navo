use navo_core::{Command, CursorMove, Location, PaneId};

use super::super::Browser;
use super::RowStatus;
use super::binding::Saved;
use super::mode::ExitStep;

impl Browser {
    /// Enter: a folder pair opens in both panes, a file row asks for the file diff.
    pub fn compare_enter(&mut self) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_ref() else {
            return false;
        };
        let Some(row) = mode.selected().cloned() else {
            return false;
        };
        if row.is_folder_pair() {
            let targets = [0, 1].map(|side| mode.side_location(side, &row.path));
            let [Some(left), Some(right)] = targets else {
                return false;
            };
            self.compare_panes([(left, None), (right, None)]);
            if let Some(mode) = self.hooks.compare.mode.as_mut() {
                mode.show_folder(row.path);
            }
            return true;
        }
        if !row.has_file() {
            return false;
        }
        self.hooks.compare.file_diff = Some(mode.file_diff(&row));
        true
    }

    /// Backspace: both panes go up and select the folder they came from; stops at the roots.
    pub fn compare_up(&mut self) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_ref() else {
            return false;
        };
        let child = mode.folder().to_owned();
        if child.is_empty() {
            return false;
        }
        let parent = child.rsplit_once('/').map_or("", |(parent, _)| parent);
        let targets = [0, 1].map(|side| mode.side_location(side, parent));
        let [Some(left), Some(right)] = targets else {
            return false;
        };
        let name = child.rsplit('/').next().map(str::to_owned);
        let parent = parent.to_owned();
        self.compare_panes([(left, name.clone()), (right, name)]);
        if let Some(mode) = self.hooks.compare.mode.as_mut() {
            mode.show_folder(parent);
            mode.select(&child);
        }
        true
    }

    /// Shows the folder of the row at `path` in both panes and selects it; the Stream calls it
    /// on the way back. False outside a compare or when a side of the folder cannot be joined.
    pub fn compare_reveal(&mut self, path: &str) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_ref() else {
            return false;
        };
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        let moves = folder != mode.folder();
        if moves {
            let targets = [0, 1].map(|side| mode.side_location(side, folder));
            let [Some(left), Some(right)] = targets else {
                return false;
            };
            let name = Some(name.to_owned());
            self.compare_panes([(left, name.clone()), (right, name)]);
        }
        let Some(mode) = self.hooks.compare.mode.as_mut() else {
            return false;
        };
        if moves {
            mode.show_folder(folder.to_owned());
        }
        let hidden = mode
            .rows
            .by_path(path)
            .is_some_and(|row| !mode.filter().keeps(row));
        if hidden {
            mode.set_filter(super::CompareFilter::All);
        }
        mode.select(path);
        true
    }

    /// Esc: clears the compare filter first, then leaves compare mode.
    pub fn compare_exit(&mut self) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_mut() else {
            return false;
        };
        if mode.exit_step() == ExitStep::Leave {
            self.leave_compare();
        }
        true
    }

    /// Leaves compare mode: the engine is cancelled and both panes return to where they were.
    /// The binding is always there with the mode, so a `None` here means no compare to leave.
    pub fn leave_compare(&mut self) {
        let Some(mode) = self.hooks.compare.mode.take() else {
            return;
        };
        let Some(binding) = self.hooks.compare.binding.take() else {
            return;
        };
        drop(mode);
        let focused = binding.focused;
        for saved in binding.saved {
            self.restore_pane(saved);
        }
        self.focus_compare_pane(focused);
    }

    pub fn compare_next_difference(&mut self) -> bool {
        self.hooks
            .compare
            .mode
            .as_mut()
            .is_some_and(super::CompareMode::next_difference)
    }

    pub fn compare_previous_difference(&mut self) -> bool {
        self.hooks
            .compare
            .mode
            .as_mut()
            .is_some_and(super::CompareMode::previous_difference)
    }

    /// Resumes a walk that stopped at the network limit; false when none waits.
    pub fn compare_continue(&mut self) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_ref() else {
            return false;
        };
        mode.continue_walk()
    }

    /// Moves the compare selection like a list cursor.
    pub fn compare_cursor(&mut self, to: CursorMove) -> bool {
        let moved = self.hooks.compare.mode.as_mut();
        moved.is_some_and(|mode| mode.move_cursor(to))
    }

    /// Selects the row with `path`, as a click does.
    pub fn compare_select(&mut self, path: &str) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_mut() else {
            return false;
        };
        mode.select(path);
        true
    }

    /// Shows only the rows of `filter`.
    pub fn compare_filter(&mut self, filter: super::CompareFilter) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_mut() else {
            return false;
        };
        mode.set_filter(filter);
        true
    }

    /// Stops the content check of the selected row while it is checked, else of all rows.
    pub fn compare_stop_checking(&mut self) -> bool {
        let Some(mode) = self.hooks.compare.mode.as_ref() else {
            return false;
        };
        match mode
            .selected()
            .filter(|row| row.status == RowStatus::Checking)
        {
            Some(row) => mode.control.stop_row(row.id),
            None => mode.control.stop_all(),
        }
        true
    }

    /// Navigates each compare pane; `name` is selected once its listing arrives.
    fn compare_panes(&mut self, targets: [(Location, Option<String>); 2]) {
        let Some(binding) = self.hooks.compare.binding.as_ref() else {
            return;
        };
        let panes = [binding.saved[0].pane, binding.saved[1].pane];
        let focused = self.state.focused_pane();
        for (pane, (location, name)) in panes.into_iter().zip(targets) {
            self.navigate_pane(pane, location, name);
        }
        self.focus_compare_pane(focused);
    }

    fn restore_pane(&mut self, saved: Saved) {
        let Saved {
            pane,
            location,
            cursor,
        } = saved;
        let here = self.state.pane(pane).map(|p| p.active().location().clone());
        if here.as_ref() != Some(&location) {
            self.navigate_pane(pane, location, cursor);
            return;
        }
        if self.focus_compare_pane(pane) {
            self.cursor_to(cursor.as_deref());
        }
    }

    fn navigate_pane(&mut self, pane: PaneId, location: Location, name: Option<String>) {
        if !self.focus_compare_pane(pane) {
            return;
        }
        let tab = self.state.active_tab().id();
        if self.command(&Command::PaneNavigate { location }).is_ok()
            && let Some(name) = name
        {
            self.focus_after_load.insert(tab, name);
        }
    }

    fn cursor_to(&mut self, name: Option<&str>) {
        let tab = self.state.active_tab();
        let row = name.and_then(|name| {
            tab.listing()?
                .entries()
                .iter()
                .position(|entry| entry.name == name)
        });
        if let Some(row) = row.and_then(|row| u64::try_from(row).ok()) {
            let _ = self.command(&Command::PaneCursorMove {
                to: CursorMove::Row(row),
            });
        }
    }

    /// False when the pane is gone from the focused window.
    fn focus_compare_pane(&mut self, pane: PaneId) -> bool {
        if self.state.focused_pane() == pane {
            return true;
        }
        let Some(window) = self.state.window(self.state.focused_window()) else {
            return false;
        };
        let Some(at) = window.layout().numbered().iter().position(|&p| p == pane) else {
            return false;
        };
        if self.state.focus_number(at + 1).is_err() {
            return false;
        }
        self.revalidate_focused();
        true
    }
}
