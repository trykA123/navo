use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};

use super::open;
use crate::primitives::Kbd;

struct Search;

impl Render for Search {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().children(Kbd::new("search-key", "ctrl-k"))
    }
}

#[test]
fn kbd_formats_its_keys_for_the_platform() {
    let text = Kbd::new("key", "ctrl-k").map(|kbd| kbd.text());
    assert_eq!(text.as_deref(), Some("Ctrl+K"));
}

#[test]
fn kbd_rejects_text_that_is_not_a_keystroke() {
    assert!(Kbd::new("key", "a-b").is_none());
}

#[gpui_kit::test]
fn kbd_shows_its_text(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Search);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let kbd = window.find("search-key");
        assert!(kbd.visible());
        assert_eq!(kbd.label(), Some("Ctrl+K"));
    })
    .expect("window");
}
