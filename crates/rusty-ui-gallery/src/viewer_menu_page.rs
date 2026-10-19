use rusty_core::shell_menu::ShellMenu;
use rusty_ui::context_menu::{ContextMenu, ContextMenuInput};
use rusty_ui::prelude::*;
use rusty_ui::settings::{Change, Field, Value, Values};
use rusty_ui::tokens;
use rusty_ui::window_view::{ViewerBody, view_menu, viewer_menu};

use crate::automotive_sample::{MEMORY_NAME, memory};
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
const GALLERY_TOP: f32 = 120.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Text,
    Tree,
    ViewText,
    ViewAutomotive,
}

pub struct ViewerMenuPage {
    shows: Shows,
    body: Option<Box<dyn ViewerBody>>,
    menu: Option<Entity<ContextMenu>>,
    frames: u8,
}

impl ViewerMenuPage {
    pub fn new(shows: Shows, window: &mut Window, cx: &mut App) -> Self {
        let body = structured::body(body_title(shows), body_text(shows), window, cx);
        if let Some(body) = &body {
            body.show_parts(&shown(shows), cx);
        }
        Self {
            shows,
            body,
            menu: None,
            frames: 0,
        }
    }

    fn shell_menu(&self, body: &dyn ViewerBody, window: &Window, cx: &mut App) -> ShellMenu {
        let focus = body.focus_handle(cx);
        match self.shows {
            Shows::Text | Shows::Tree => viewer_menu(&body.menu(false, cx), &focus, window),
            Shows::ViewText | Shows::ViewAutomotive => {
                view_menu(body, &shown(self.shows), window, cx).unwrap_or_default()
            }
        }
    }

    fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.body.as_ref().filter(|_| self.menu.is_none()) else {
            return;
        };
        let menu = self.shell_menu(body.as_ref(), window, cx);
        let focus = body.focus_handle(cx);
        let width = window.viewport_size().width;
        let anchor = match self.shows {
            Shows::Text => Point::new(px(360.), px(250.)),
            Shows::Tree => Point::new(px(420.), px(210.)),
            Shows::ViewText | Shows::ViewAutomotive => Point::new(width - px(24.), px(GALLERY_TOP)),
        };
        let input = ContextMenuInput {
            menu,
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
        Shows::Text | Shows::ViewText => TEXT_NAME,
        Shows::Tree => TREE_NAME,
        Shows::ViewAutomotive => MEMORY_NAME,
    }
}

fn body_text(shows: Shows) -> &'static str {
    match shows {
        Shows::Text | Shows::ViewText => TEXT,
        Shows::Tree => TREE,
        Shows::ViewAutomotive => memory(),
    }
}

fn shown(shows: Shows) -> Values {
    let flag = |field, on| Change {
        field,
        value: Value::Flag(on),
    };
    match shows {
        Shows::ViewAutomotive => Values::default()
            .with(&Change {
                field: Field::AutomotivePreset,
                value: Value::Pick(2),
            })
            .with(&flag(Field::AutomotiveInspector, true))
            .with(&flag(Field::AutomotiveRowBars, true)),
        _ => Values::default(),
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
