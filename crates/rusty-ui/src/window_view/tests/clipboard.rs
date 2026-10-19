use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::{Location, OpKind};
use rusty_fs::{FakeFs, Provider};

use super::{fake, home_beside_dest, open_options, options_for, press};
use crate::browser::{JobStatus, Operations};
use crate::window_view::{
    ClipboardEffect, ClipboardFault, ClipboardFiles, FerriteWindow, FileClipboardService,
};

#[derive(Default)]
pub(in crate::window_view) struct FakeClipboard {
    pub held: RefCell<Option<ClipboardFiles>>,
    pub writes: RefCell<Vec<ClipboardFiles>>,
    pub clears: RefCell<Vec<ClipboardFiles>>,
    pub unsupported: bool,
}

impl FileClipboardService for FakeClipboard {
    fn write(&self, files: &ClipboardFiles) -> Result<(), ClipboardFault> {
        if self.unsupported {
            return Err(ClipboardFault::Unsupported);
        }
        self.writes.borrow_mut().push(files.clone());
        *self.held.borrow_mut() = Some(files.clone());
        Ok(())
    }

    fn read(&self) -> Result<Option<ClipboardFiles>, ClipboardFault> {
        if self.unsupported {
            return Err(ClipboardFault::Unsupported);
        }
        Ok(self.held.borrow().clone())
    }

    fn clear_if_same(&self, files: &ClipboardFiles) -> Result<bool, ClipboardFault> {
        if self.unsupported {
            return Err(ClipboardFault::Unsupported);
        }
        self.clears.borrow_mut().push(files.clone());
        let mut held = self.held.borrow_mut();
        let is_same = held.as_ref() == Some(files);
        if is_same {
            *held = None;
        }
        Ok(is_same)
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    fs: Arc<FakeFs>,
    clipboard: Rc<FakeClipboard>,
}

fn rig(cx: &mut TestAppContext, clipboard: FakeClipboard) -> Rig {
    rig_guarding(cx, clipboard, &[])
}

fn rig_guarding(cx: &mut TestAppContext, clipboard: FakeClipboard, protected: &[&str]) -> Rig {
    let fs = fake();
    let clipboard = Rc::new(clipboard);
    let shared: Rc<dyn FileClipboardService> = clipboard.clone();
    let operations = Operations {
        protected: protected.iter().map(|path| fs.location(path)).collect(),
        ..Operations::unlogged()
    };
    let options = crate::window_view::StartOptions {
        file_clipboard: Some(shared),
        operations,
        ..options_for(fs.clone(), home_beside_dest(&fs))
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        fs,
        clipboard,
    }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            view.status_notice().map(|notice| notice.text.to_string())
        })
    }

    fn writes(&self) -> Vec<ClipboardFiles> {
        self.clipboard.writes.borrow().clone()
    }

    fn at(&self, path: &str) -> Location {
        self.fs.location(path)
    }

    fn hold(&self, paths: &[&str], effect: ClipboardEffect) {
        let items = paths.iter().map(|path| self.at(path)).collect();
        *self.clipboard.held.borrow_mut() = Some(ClipboardFiles { items, effect });
    }

    fn jobs(&self, cx: &mut TestAppContext) -> Vec<(OpKind, Option<Location>, JobStatus)> {
        self.view.read_with(cx, |view, _| {
            view.browser
                .jobs()
                .iter()
                .map(|job| (job.kind, job.target.clone(), job.status.clone()))
                .collect()
        })
    }

    fn has_pending(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.browser.pending().is_some())
    }

    fn exists(&self, path: &str) -> bool {
        self.fs.metadata(&self.at(path)).is_ok()
    }
}

fn mark_both(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "space");
    rig.press(cx, "down");
    rig.press(cx, "space");
}

#[gpui_kit::test]
fn ctrl_c_writes_the_marked_items_with_copy(cx: &mut TestAppContext) {
    let rig = rig(cx, FakeClipboard::default());
    mark_both(cx, &rig);
    rig.press(cx, "ctrl-c");
    assert_eq!(
        rig.writes(),
        [ClipboardFiles {
            items: vec![rig.at("home/docs"), rig.at("home/a.txt")],
            effect: ClipboardEffect::Copy,
        }]
    );
    assert_eq!(
        rig.notice(cx).as_deref(),
        Some("Copied 2 items to the clipboard")
    );
}

#[gpui_kit::test]
fn ctrl_x_writes_the_marked_items_with_move(cx: &mut TestAppContext) {
    let rig = rig(cx, FakeClipboard::default());
    mark_both(cx, &rig);
    rig.press(cx, "ctrl-x");
    assert_eq!(
        rig.writes(),
        [ClipboardFiles {
            items: vec![rig.at("home/docs"), rig.at("home/a.txt")],
            effect: ClipboardEffect::Move,
        }]
    );
    assert_eq!(
        rig.notice(cx).as_deref(),
        Some("Cut 2 items to the clipboard")
    );
}

#[gpui_kit::test]
fn with_nothing_marked_the_cursor_item_alone_goes_to_the_clipboard(cx: &mut TestAppContext) {
    let rig = rig(cx, FakeClipboard::default());
    rig.press(cx, "down");
    rig.press(cx, "ctrl-c");
    let writes = rig.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].items, [rig.at("home/a.txt")]);
    assert!(!writes[0].items.contains(&rig.at("")));
    assert_eq!(
        rig.notice(cx).as_deref(),
        Some("Copied 1 item to the clipboard")
    );
}

#[gpui_kit::test]
fn ctrl_v_with_copied_files_copies_them_into_the_active_folder(cx: &mut TestAppContext) {
    let rig = rig(cx, FakeClipboard::default());
    rig.hold(&["home/a.txt"], ClipboardEffect::Copy);
    rig.press(cx, "tab");
    rig.press(cx, "ctrl-v");
    assert!(!rig.has_pending(cx));
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Copy, Some(rig.at("dest")), JobStatus::Done)]
    );
    assert!(rig.exists("dest/a.txt"));
    assert!(rig.exists("home/a.txt"));
    assert!(rig.clipboard.clears.borrow().is_empty());
}

#[gpui_kit::test]
fn ctrl_v_with_cut_files_moves_them_and_empties_the_clipboard(cx: &mut TestAppContext) {
    let rig = rig(cx, FakeClipboard::default());
    rig.hold(&["home/a.txt"], ClipboardEffect::Move);
    rig.press(cx, "tab");
    rig.press(cx, "ctrl-v");
    assert!(!rig.has_pending(cx));
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.at("dest")), JobStatus::Done)]
    );
    assert!(rig.exists("dest/a.txt"));
    assert!(!rig.exists("home/a.txt"));
    let cut = ClipboardFiles {
        items: vec![rig.at("home/a.txt")],
        effect: ClipboardEffect::Move,
    };
    assert_eq!(*rig.clipboard.clears.borrow(), [cut]);
    assert_eq!(*rig.clipboard.held.borrow(), None);
}

#[gpui_kit::test]
fn ctrl_v_with_nothing_on_the_clipboard_says_so_and_starts_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx, FakeClipboard::default());
    rig.press(cx, "tab");
    rig.press(cx, "ctrl-v");
    assert_eq!(rig.notice(cx).as_deref(), Some("Nothing to paste"));
    assert!(rig.jobs(cx).is_empty());
    assert!(!rig.has_pending(cx));
}

#[gpui_kit::test]
fn without_a_file_clipboard_the_keys_say_it_works_on_windows_only(cx: &mut TestAppContext) {
    let unsupported = FakeClipboard {
        unsupported: true,
        ..FakeClipboard::default()
    };
    let rig = rig(cx, unsupported);
    rig.press(cx, "ctrl-c");
    let windows_only = Some("The file clipboard works on Windows only");
    assert_eq!(rig.notice(cx).as_deref(), windows_only);
    rig.press(cx, "down");
    assert_eq!(rig.notice(cx), None);
    rig.press(cx, "ctrl-v");
    assert_eq!(rig.notice(cx).as_deref(), windows_only);
    assert!(rig.jobs(cx).is_empty());
}

#[gpui_kit::test]
fn pasting_a_cut_protected_folder_still_asks_for_the_typed_word(cx: &mut TestAppContext) {
    let rig = rig_guarding(cx, FakeClipboard::default(), &["home/docs"]);
    rig.hold(&["home/docs"], ClipboardEffect::Move);
    rig.press(cx, "tab");
    rig.press(cx, "ctrl-v");
    let typed = rig.view.read_with(cx, |view, _| {
        view.browser
            .pending()
            .and_then(|pending| super::super::operate::confirm_props(pending).typed)
    });
    assert_eq!(typed.as_deref(), Some("DELETE"));
    assert!(rig.jobs(cx).is_empty());
    assert!(rig.exists("home/docs/report.txt"));
}
