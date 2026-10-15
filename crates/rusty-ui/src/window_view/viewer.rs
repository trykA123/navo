use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::{Focusable, KeyBinding, Subscription};
use rusty_core::{Command, Location};

use super::FerriteWindow;
use crate::prelude::*;
use crate::tokens;

const CONTEXT: &str = "Viewer";

gpui_kit::actions!(viewer, [SaveFile, CloseViewer]);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("ctrl-s", SaveFile, context),
        KeyBinding::new("escape", CloseViewer, context),
        KeyBinding::new("f3", CloseViewer, context),
    ]);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileText {
    pub text: String,
    pub read_only: bool,
}

type Read = Rc<dyn Fn(&Location) -> Result<FileText, String>>;
type Write = Rc<dyn Fn(&Location, &str) -> Result<(), String>>;

pub struct FileHooks {
    pub read: Read,
    pub write: Write,
}

impl FileHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            read: Rc::new(|_| Err("Files cannot be opened here.".to_owned())),
            write: Rc::new(|_, _| Err("Files cannot be saved here.".to_owned())),
        }
    }
}

pub(super) struct Viewer {
    location: Location,
    title: SharedString,
    editor: Option<Entity<EditorState>>,
    saved: SharedString,
    read_only: bool,
    message: Option<SharedString>,
    leaving: bool,
    _changes: Option<Subscription>,
}

pub(super) fn language_of(name: &str) -> &str {
    let ext = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or("", |(_, ext)| ext);
    match ext.to_ascii_lowercase().as_str() {
        "h" => "c",
        "hpp" | "hh" | "cc" | "cxx" => "cpp",
        "jsx" | "mjs" | "cjs" => "javascript",
        "mts" | "cts" => "typescript",
        "htm" | "xhtml" => "html",
        "sh" | "zsh" => "bash",
        "kts" | "kt" => "kotlin",
        "cs" => "csharp",
        "py" | "pyi" => "python",
        "rs" => "rust",
        "md" | "mdx" => "markdown",
        "yml" | "yaml" => "yaml",
        "js" => "javascript",
        "ts" => "typescript",
        "rb" => "ruby",
        "toml" | "json" | "css" | "html" | "go" | "java" | "lua" | "php" | "proto" | "scala"
        | "swift" | "zig" | "c" | "cpp" | "tsx" | "svelte" | "astro" | "diff" | "graphql"
        | "ejs" | "erb" | "cmake" => ext,
        _ => "text",
    }
}

impl FerriteWindow {
    pub(super) fn view_row(&mut self, row: u64, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.browser.state().active_tab();
        let found = tab
            .listing()
            .and_then(|listing| listing.get(row))
            .filter(|entry| !entry.is_folder)
            .and_then(|entry| {
                let name = entry.name.clone();
                let location = tab
                    .location()
                    .join(&rusty_core::ProviderPath::from_text(&name))
                    .ok()?;
                Some((name, location))
            });
        let Some((_, location)) = found else {
            return;
        };
        self.view_location(location, window, cx);
    }

    pub(super) fn view_location(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = location
            .name()
            .map_or_else(|| location.as_str().to_owned(), |name| name.to_text());
        let viewer = match (self.files.read)(&location) {
            Ok(file) => {
                let opened = Opened {
                    location,
                    name,
                    file,
                };
                self.viewer_for(opened, window, cx)
            }
            Err(message) => Viewer {
                location,
                title: name.into(),
                editor: None,
                saved: SharedString::default(),
                read_only: true,
                message: Some(message.into()),
                leaving: false,
                _changes: None,
            },
        };
        self.viewer = Some(viewer);
        cx.notify();
    }

    fn viewer_for(
        &mut self,
        opened: Opened,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Viewer {
        let Opened {
            location,
            name,
            file,
        } = opened;
        let name = name.as_str();
        let saved: SharedString = file.text.into();
        let text = saved.clone();
        let language = language_of(name).to_owned();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language)
                .default_value(text)
        });
        let changes = cx.subscribe(&editor, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.text_changed(cx);
            }
        });
        let handle = editor.read(cx).focus_handle(cx);
        window.defer(cx, move |window, cx| window.focus(&handle, cx));
        Viewer {
            location,
            title: name.to_owned().into(),
            editor: Some(editor),
            saved,
            read_only: file.read_only,
            message: file
                .read_only
                .then(|| "Not valid UTF-8: opened read-only.".into()),
            leaving: false,
            _changes: Some(changes),
        }
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = &mut self.viewer {
            viewer.leaving = false;
        }
        cx.notify();
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.viewer.as_ref().is_some_and(|viewer| {
            viewer
                .editor
                .as_ref()
                .is_some_and(|editor| editor.read(cx).value() != viewer.saved)
        })
    }

    fn save_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        let Some(editor) = viewer.editor.clone().filter(|_| !viewer.read_only) else {
            return;
        };
        let text = editor.read(cx).value();
        let result = (self.files.write)(&viewer.location, &text);
        if let Some(viewer) = &mut self.viewer {
            match result {
                Ok(()) => {
                    viewer.saved = text;
                    viewer.message = Some("Saved.".into());
                }
                Err(error) => viewer.message = Some(error.into()),
            }
        }
        let _ = self.browser.command(&Command::PaneRefresh);
        cx.notify();
    }

    fn close_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dirty = self.is_dirty(cx);
        if let Some(viewer) = &mut self.viewer
            && dirty
            && !viewer.leaving
        {
            viewer.leaving = true;
            viewer.message = Some("Unsaved changes. Ctrl+S saves; Esc again discards them.".into());
            cx.notify();
            return;
        }
        self.viewer = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    pub(super) fn viewer_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let viewer = self.viewer.as_ref()?;
        let t = tokens::current(cx).clone();
        let dirty = self.is_dirty(cx);
        let header = viewer_header(&t, viewer, dirty);
        let body = viewer_body(&t, viewer);
        let (save, close) = (
            cx.listener(|view, _: &SaveFile, _, cx| view.save_viewer(cx)),
            cx.listener(|view, _: &CloseViewer, window, cx| view.close_viewer(window, cx)),
        );
        Some(
            div()
                .id("viewer")
                .key_context(CONTEXT)
                .on_action(save)
                .on_action(close)
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .bg(t.colors.surface)
                .child(header)
                .child(body)
                .into_any_element(),
        )
    }
}

struct Opened {
    location: Location,
    name: String,
    file: FileText,
}

fn viewer_header(t: &tokens::Theme, viewer: &Viewer, dirty: bool) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .h(t.spacing.path_height)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(
            div()
                .id("viewer-title")
                .test_support()
                .font_weight(FontWeight::SEMIBOLD)
                .child(viewer.title.clone()),
        )
        .when(dirty, |d| {
            d.child(div().text_color(t.colors.mark).child("●"))
        })
        .child(div().flex_1())
        .when_some(viewer.message.clone(), |d, message| {
            d.child(
                div()
                    .id("viewer-message")
                    .test_support()
                    .text_color(t.colors.muted)
                    .text_size(t.typography.small)
                    .child(message),
            )
        })
        .child(
            div()
                .text_color(t.colors.faint)
                .text_size(t.typography.small)
                .child("Ctrl+F find · Ctrl+S save · Esc close"),
        )
}

fn viewer_body(t: &tokens::Theme, viewer: &Viewer) -> Div {
    match &viewer.editor {
        Some(editor) => div()
            .flex_1()
            .min_h_0()
            .child(Editor::new(editor).h_full().readonly(viewer.read_only)),
        None => div()
            .flex_1()
            .p(t.spacing.xxl)
            .text_color(t.colors.muted)
            .child(
                viewer
                    .message
                    .clone()
                    .unwrap_or_else(|| "Nothing to show.".into()),
            ),
    }
}
