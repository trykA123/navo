use rusty_ui::context_menu::{ContextMenu, ContextMenuInput};
use rusty_ui::prelude::*;
use rusty_ui::tokens;
use rusty_ui::window_view::{ViewerBody, viewer_menu};

use crate::structured_sample::{self as structured, TREE, TREE_NAME};

const TEXT_NAME: &str = "deploy.sh";
const TEXT: &str = "#!/usr/bin/env bash
set -euo pipefail

# Build the release and copy it next to the old one.
target=\"dist/ferrite-$(date +%Y%m%d)\"
cargo build --release
mkdir -p \"$target\"
cp target/release/ferrite \"$target/\"
echo \"Copied to $target\"
";
const FRAMES_BEFORE_OPEN: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Text,
    Tree,
}

pub struct ViewerMenuPage {
    shows: Shows,
    body: Option<Box<dyn ViewerBody>>,
    menu: Option<Entity<ContextMenu>>,
    frames: u8,
}

impl ViewerMenuPage {
    pub fn new(shows: Shows, window: &mut Window, cx: &mut App) -> Self {
        let (name, text) = match shows {
            Shows::Text => (TEXT_NAME, TEXT),
            Shows::Tree => (TREE_NAME, TREE),
        };
        Self {
            shows,
            body: structured::body(name, text, window, cx),
            menu: None,
            frames: 0,
        }
    }

    fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.body.as_ref().filter(|_| self.menu.is_none()) else {
            return;
        };
        let entries = body.menu(false, cx);
        let focus = body.focus_handle(cx);
        let anchor = match self.shows {
            Shows::Text => Point::new(px(360.), px(250.)),
            Shows::Tree => Point::new(px(420.), px(210.)),
        };
        let input = ContextMenuInput {
            menu: viewer_menu(&entries, &focus, window),
            context: body_title(self.shows).into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(focus),
        };
        self.menu = Some(cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        }));
        cx.notify();
    }
}

fn body_title(shows: Shows) -> &'static str {
    match shows {
        Shows::Text => TEXT_NAME,
        Shows::Tree => TREE_NAME,
    }
}

impl Render for ViewerMenuPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        if self.menu.is_none() {
            if self.frames < FRAMES_BEFORE_OPEN {
                self.frames += 1;
                window.request_animation_frame();
            } else {
                cx.defer_in(window, Self::open_menu);
            }
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .children(self.body.as_ref().map(|body| body.view()))
            .children(self.menu.clone())
    }
}
