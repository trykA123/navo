//! Key moves of the Folders view: up and down, step in, step out and Enter. A child module of
//! `folders_view`, so it reads the view's private fields.

use super::{FoldersEvent, FoldersView};
use gpui_kit::Context;

impl FoldersView {
    /// Up (-1) or down (+1) one line, stopping at the ends.
    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let lines = self.tree.lines();
        let at = self.cursor.saturating_add_signed(delta);
        if at < lines.len() {
            self.move_to(at, &lines, cx);
        }
    }

    /// Right: opens a closed folder, or moves to the first child of an open one.
    pub(super) fn step_in(&mut self, cx: &mut Context<Self>) {
        let Some((lines, line)) = self.current().filter(|(_, line)| line.folder) else {
            return;
        };
        match (line.expanded, lines.get(self.cursor + 1)) {
            (false, _) => self.set_open(&line.path, true, cx),
            (true, Some(next)) if next.depth > line.depth => {
                self.move_to(self.cursor + 1, &lines, cx)
            }
            _ => {}
        }
    }

    /// Left: closes an open folder, or moves to the parent line.
    pub(super) fn step_out(&mut self, cx: &mut Context<Self>) {
        let Some((lines, line)) = self.current() else {
            return;
        };
        if line.folder && line.expanded {
            return self.set_open(&line.path, false, cx);
        }
        let parent = line.path.rsplit_once('/').map(|(parent, _)| parent);
        let at = lines
            .iter()
            .position(|other| Some(other.path.as_str()) == parent);
        if let Some(at) = at {
            self.move_to(at, &lines, cx);
        }
    }

    /// Enter: toggles a folder, or asks the parent to open the file pair.
    pub(super) fn activate(&mut self, cx: &mut Context<Self>) {
        let Some((_, line)) = self.current() else {
            return;
        };
        match line.folder {
            true => self.set_open(&line.path, !line.expanded, cx),
            false => cx.emit(FoldersEvent::OpenPair(line.id)),
        }
    }
}
