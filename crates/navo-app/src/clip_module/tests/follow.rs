use std::cell::Cell;
use std::rc::Rc;

use navo_ui::clip_palette::ClipboardHooks;
use navo_ui::settings::Values;
use navo_ui::window_view::ResidentHooks;

use super::super::follow;
use super::Rig;

#[derive(Default)]
struct Inner {
    applied: Cell<usize>,
}

impl ResidentHooks for Inner {
    fn apply(&self, _values: &Values) {
        self.applied.set(self.applied.get() + 1);
    }

    fn key_taken(&self) -> bool {
        true
    }

    fn clipboard_key_taken(&self) -> bool {
        true
    }

    fn has_tray(&self) -> bool {
        true
    }
}

#[test]
fn follow_answers_from_the_inner_hooks() {
    let rig = Rig::new();
    let inner: Rc<dyn ResidentHooks> = Rc::new(Inner::default());
    let hooks = follow(&rig.module, Some(inner));
    assert!(hooks.key_taken());
    assert!(hooks.clipboard_key_taken());
    assert!(hooks.has_tray());
    let alone = follow(&rig.module, None);
    assert!(!alone.key_taken() && !alone.clipboard_key_taken() && !alone.has_tray());
}

#[test]
fn follow_applies_settings_to_the_inner_hooks_and_the_module() {
    let rig = Rig::new();
    let inner = Rc::new(Inner::default());
    let hooks = follow(&rig.module, Some(inner.clone() as Rc<dyn ResidentHooks>));
    hooks.apply(&Values::default());
    rig.module.settle();
    assert_eq!(inner.applied.get(), 1);
    assert_eq!(rig.module.modes().len(), 5);
}
