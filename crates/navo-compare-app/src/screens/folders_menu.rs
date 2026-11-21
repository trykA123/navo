//! The context menu of a Folders line (NC136, NC137): its lines, the copy handlers and the
//! `Shift+F10` anchor. A child of `folders_view`, which owns the state the handlers change.

use std::path::{Path, PathBuf};

use gpui_kit::{Action, AppContext as _, ClipboardItem};
use gpui_kit::{Context, Div, InteractiveElement as _, Pixels, Point, Window};
use sidewise_host::compare::hash_cache::file::ClearError;
use sidewise_kit::format::bytes as grouped;
use sidewise_l10n::t;

use super::{Activate, FoldersEvent, FoldersView};
use crate::layers::toast;
use crate::menu_host::{MenuAt, MenuLine, show_menu};
use crate::screens::folders_model::TreeLine;
use crate::screens::host_parts::clear_hashes;
use crate::screens::reveal::reveal;

gpui_kit::actions!(
    folders,
    [
        OpenStream,
        CopyLeftPath,
        CopyRightPath,
        CopyName,
        RevealLeft,
        RevealRight,
        ExpandAll,
        ClearCache,
        ShowMenu
    ]
);

/// The absolute path of `rel` (a `/`-separated row path) under `root`.
pub(super) fn side_path(root: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|part| !part.is_empty())
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

/// One runnable menu line.
fn run(label: &str, action: impl Action, enabled: bool) -> MenuLine {
    MenuLine::Run {
        label: label.to_owned().into(),
        action: Box::new(action),
        enabled,
    }
}

/// The menu lines of `line`: open, a break, the copies and the Reveal lines, a break, then the cache lines. A copy of a side is dimmed when
/// that side is missing or the compare gave no roots (`has_roots` false); Reveal also needs Windows.
pub(super) fn lines_for(line: &TreeLine, has_roots: bool) -> Vec<MenuLine> {
    let reveal_ok = has_roots && cfg!(windows);
    let mut lines = match (line.folder, line.expanded) {
        (false, _) => vec![
            run("Open diff", Activate, true),
            run("Open in Stream", OpenStream, true),
        ],
        (true, open) => vec![
            run(if open { "Collapse" } else { "Expand" }, Activate, true),
            run("Expand all", ExpandAll, true),
        ],
    };
    lines.extend([
        MenuLine::Break,
        run(
            "Copy left path",
            CopyLeftPath,
            has_roots && line.left.is_some(),
        ),
        run(
            "Copy right path",
            CopyRightPath,
            has_roots && line.right.is_some(),
        ),
        run("Copy name", CopyName, true),
        run(
            "Reveal left in Explorer",
            RevealLeft,
            reveal_ok && line.left.is_some(),
        ),
        run(
            "Reveal right in Explorer",
            RevealRight,
            reveal_ok && line.right.is_some(),
        ),
        MenuLine::Break,
        run("Recheck all", super::super::compare_tab_keys::Refresh, true),
        run("Clear compare cache", ClearCache, true),
    ]);
    lines
}

impl FoldersView {
    /// Opens the menu of the line under the cursor at `anchor`; an empty tree does nothing.
    fn show_menu_at(&mut self, anchor: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, line)) = self.current() else {
            return;
        };
        let lines = lines_for(&line, self.roots().is_some());
        let at = MenuAt {
            anchor,
            focus: self.focus.clone(),
            title: "Folders line",
        };
        show_menu(self, (&lines, at, |this| &mut this.menu), window, cx);
    }

    /// A right click on line `at`: the cursor goes there, then the menu opens at the pointer.
    pub(crate) fn menu_at_pointer(
        &mut self,
        (at, pointer): (usize, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.click_line(at, cx);
        self.show_menu_at(pointer, window, cx);
    }

    /// `Shift+F10`: the menu opens below the cursor line's left edge.
    fn show_menu_at_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = match self.cursor_bounds.get() {
            Some(bounds) => bounds.bottom_left(),
            None => window.mouse_position(),
        };
        self.show_menu_at(anchor, window, cx);
    }

    /// Asks the parent to open the file under the cursor as a one-card Stream; folders are ignored.
    fn open_stream(&mut self, cx: &mut Context<Self>) {
        if let Some((_, line)) = self.current().filter(|(_, line)| !line.folder) {
            cx.emit(FoldersEvent::OpenStream(line.id));
        }
    }

    /// On a Git side there is no file on disk: toasts why and returns `true`.
    fn git_side_toast(&self, side: usize, cx: &mut Context<Self>) -> bool {
        let rev = self.roots().and_then(|roots| roots[side].rev.clone());
        match rev {
            Some(rev) => {
                toast::notify(t!("git-side-no-file", rev = rev), cx);
                true
            }
            None => false,
        }
    }

    /// Copies the absolute path of the cursor line on side `side` (0 left, 1 right) and toasts.
    fn copy_side(&mut self, side: usize, cx: &mut Context<Self>) {
        if self.git_side_toast(side, cx) {
            return;
        }
        let Some((_, line)) = self.current() else {
            return;
        };
        let present = [&line.left, &line.right][side].is_some();
        let Some(root) = self
            .roots()
            .filter(|_| present)
            .map(|roots| &roots[side].path)
        else {
            return;
        };
        let path = side_path(root, &line.path).to_string_lossy().into_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(path));
        let word = ["left", "right"][side];
        toast::notify(format!("Copied {word} path"), cx);
    }

    /// Reveals the cursor line on side `side` in Explorer; a failure or non-Windows toasts why.
    fn reveal_side(&mut self, side: usize, cx: &mut Context<Self>) {
        if self.git_side_toast(side, cx) {
            return;
        }
        let Some((_, line)) = self.current() else {
            return;
        };
        let present = [&line.left, &line.right][side].is_some();
        let Some(root) = self
            .roots()
            .filter(|_| present)
            .map(|roots| &roots[side].path)
        else {
            return;
        };
        if let Err(text) = reveal(&side_path(root, &line.path)) {
            toast::notify(text, cx);
        }
    }

    /// Forgets every stored hash on a background task, then toasts the outcome.
    fn clear_cache(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |_, cx| {
            let result = cx.background_spawn(async { clear_hashes() }).await;
            let _ = cx.update(|cx| toast::notify(cache_toast(&result), cx));
        })
        .detach();
    }

    /// Copies the name of the cursor line and toasts.
    fn copy_name(&mut self, cx: &mut Context<Self>) {
        if let Some((_, line)) = self.current() {
            cx.write_to_clipboard(ClipboardItem::new_string(line.name));
            toast::notify("Copied name", cx);
        }
    }
}

/// The toast text for the end of a cache clear (NC211): the count, "already empty", busy or the reason.
pub(super) fn cache_toast(result: &Result<usize, ClearError>) -> String {
    match result {
        Ok(0) => t!("compare-cache-none"),
        Ok(count) => t!(
            "compare-cache-cleared",
            count = *count,
            shown = grouped(*count as u64)
        ),
        Err(ClearError::Busy) => t!("compare-cache-busy"),
        Err(ClearError::Io(text)) => t!("compare-cache-clear-failed", reason = text.as_str()),
    }
}

/// Wires the menu actions onto the view's root element.
pub(super) fn menu_actions(root: Div, cx: &mut Context<FoldersView>) -> Div {
    root.on_action(cx.listener(|this, _: &OpenStream, _, cx| this.open_stream(cx)))
        .on_action(cx.listener(|this, _: &CopyLeftPath, _, cx| this.copy_side(0, cx)))
        .on_action(cx.listener(|this, _: &CopyRightPath, _, cx| this.copy_side(1, cx)))
        .on_action(cx.listener(|this, _: &RevealLeft, _, cx| this.reveal_side(0, cx)))
        .on_action(cx.listener(|this, _: &RevealRight, _, cx| this.reveal_side(1, cx)))
        .on_action(cx.listener(|this, _: &ClearCache, _, cx| this.clear_cache(cx)))
        .on_action(cx.listener(|this, _: &CopyName, _, cx| this.copy_name(cx)))
        .on_action(cx.listener(|this, _: &ShowMenu, window, cx| this.show_menu_at_line(window, cx)))
}

#[cfg(test)]
mod tests {
    use super::side_path;
    use std::path::{Path, PathBuf};

    /// Each `/` segment of a row path is joined under the root.
    #[test]
    fn side_path_joins_each_segment() {
        let want = PathBuf::from("/r").join("a").join("b.txt");
        assert_eq!(side_path(Path::new("/r"), "a/b.txt"), want);
    }
}
