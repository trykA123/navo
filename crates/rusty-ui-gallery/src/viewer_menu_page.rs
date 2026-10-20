use rusty_core::shell_menu::ShellMenu;
use rusty_ui::context_menu::{ContextMenu, ContextMenuInput};
use rusty_ui::prelude::*;
use rusty_ui::settings::{Change, Field, Value, Values};
use rusty_ui::tokens;
use rusty_ui::window_view::{ViewerBody, encoding_picker, view_menu, viewer_menu};

use crate::automotive_sample::{ELF_NAME, MEMORY_NAME, TRACE_NAME, elf, memory, trace};
use crate::structured_sample::{self as structured, MARKDOWN, MARKDOWN_NAME, TREE, TREE_NAME};

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
    Encoding,
    ViewAutomotive,
    ViewElf,
    ViewTrace,
    ViewMarkdown,
}

impl Shows {
    fn is_view(self) -> bool {
        !matches!(self, Self::Text | Self::Tree | Self::Encoding)
    }
}

pub struct ViewerMenuPage {
    shows: Shows,
    body: Option<Box<dyn ViewerBody>>,
    menu: Option<Entity<ContextMenu>>,
    frames: u8,
}

impl ViewerMenuPage {
    pub fn new(shows: Shows, window: &mut Window, cx: &mut App) -> Self {
        let body = open_body(shows, window, cx);
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
            Shows::Encoding => body
                .encoding(cx)
                .map(|found| encoding_picker(&found, &found))
                .unwrap_or_default(),
            _ => view_menu(body, &shown(self.shows), window, cx).unwrap_or_default(),
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
            _ => Point::new(width - px(24.), px(GALLERY_TOP)),
        };
        let input = ContextMenuInput {
            menu,
            context: body_title(self.shows).into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(focus),
        };
        let is_view = self.shows.is_view();
        let is_encoding = self.shows == Shows::Encoding;
        self.menu = Some(cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            if is_view {
                menu.name_ferrite_group("View", cx);
            } else if is_encoding {
                menu.name_ferrite_group("Encoding", cx);
            }
            menu
        }));
        cx.notify();
    }
}

fn body_title(shows: Shows) -> &'static str {
    match shows {
        Shows::Text | Shows::ViewText | Shows::Encoding => TEXT_NAME,
        Shows::Tree => TREE_NAME,
        Shows::ViewAutomotive => MEMORY_NAME,
        Shows::ViewElf => ELF_NAME,
        Shows::ViewTrace => TRACE_NAME,
        Shows::ViewMarkdown => MARKDOWN_NAME,
    }
}

fn open_body(shows: Shows, window: &mut Window, cx: &mut App) -> Option<Box<dyn ViewerBody>> {
    match shows {
        Shows::ViewElf => structured::binary(ELF_NAME, elf(), window, cx),
        Shows::ViewMarkdown => structured::editing(MARKDOWN_NAME, MARKDOWN, window, cx),
        _ => structured::body(body_title(shows), body_text(shows), window, cx),
    }
}

fn body_text(shows: Shows) -> &'static str {
    match shows {
        Shows::Text | Shows::ViewText | Shows::Encoding => TEXT,
        Shows::Tree => TREE,
        Shows::ViewAutomotive => memory(),
        Shows::ViewElf | Shows::ViewMarkdown => "",
        Shows::ViewTrace => trace(),
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
        Shows::ViewElf => Values::default().with(&Change {
            field: Field::AutomotivePreset,
            value: Value::Pick(1),
        }),
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
