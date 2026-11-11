//! The file diff's syntax parse (D38): each side of an opened text is parsed off the UI thread
//! with F3's highlighter, once per opened text, and kept on the view for the rows to colour.

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::Task;
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::theme::Theme as KitTheme;

use crate::compare_diff::PairText;
use crate::compare_diff::body::{PairSyntax, SideSyntax};
use crate::prelude::*;
use crate::window_view::{EDIT_LIMIT, NavoWindow};

/// How many bytes of a side's first lines the language sniffer reads.
const HEAD: usize = 1024;

/// The parse of one opened text: not started, running (its task), or done.
#[derive(Default)]
pub(super) struct SyntaxSlot {
    started: bool,
    ready: Option<Rc<PairSyntax>>,
    /// The parse of the text this pair showed before a reload, kept so the rows keep their
    /// colours until the new text's own parse lands (W7f).
    provisional: Option<Rc<PairSyntax>>,
    /// The wait for the background parse; dropping it drops the result.
    task: Option<Task<()>>,
}

impl SyntaxSlot {
    /// The finished parse and the theme's highlight colours (as F3 reads them) for one frame.
    pub(super) fn parts(&self, cx: &App) -> (Option<Rc<PairSyntax>>, Option<Arc<HighlightTheme>>) {
        let colours = cx.try_global::<KitTheme>();
        let colours = colours.map(|kit| Arc::clone(&kit.highlight_theme));
        (self.shown().cloned(), colours)
    }

    /// The parse the rows colour with: this text's own once it landed, else the one kept from
    /// before a reload of the same pair.
    fn shown(&self) -> Option<&Rc<PairSyntax>> {
        self.ready.as_ref().or(self.provisional.as_ref())
    }

    /// Keeps the parse `old` was colouring with, the same pair before a reload, until this text's
    /// own one lands: its finished parse, or the provisional one it was itself still waiting for.
    pub(super) fn keep_from(&mut self, old: &Self) {
        self.provisional = old.shown().cloned();
    }
}

/// Parses one side of a file named `name`; plain over the cap or for a language with no grammar.
fn parse_side(name: &str, lines: &[String]) -> Option<SideSyntax> {
    let mut head = String::new();
    for line in lines {
        if head.len() >= HEAD {
            break;
        }
        head.push_str(line);
        head.push('\n');
    }
    let language = crate::languages::detect(name, head.as_bytes());
    SideSyntax::parse(language, lines, EDIT_LIMIT)
}

/// Parses both sides of `text`, the pair at `path`. Blocks: runs on a background thread.
fn parse_pair(path: &str, text: &PairText) -> PairSyntax {
    crate::languages::register_languages();
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    PairSyntax {
        left: parse_side(name, &text.left),
        right: parse_side(name, &text.right),
    }
}

impl NavoWindow {
    /// Starts the open text pair's syntax parse on a background thread, once per opened text.
    /// When it ends, the rows redraw in colour if that same text is still open.
    pub(in crate::window_view::compare_screen) fn start_file_syntax(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        let (tag, path) = (file.tag, file.request.path.clone());
        let Some(view) = file.view.as_mut().filter(|view| !view.syntax.started) else {
            return;
        };
        view.syntax.started = true;
        let text = Arc::clone(view.text());
        let parsed = Arc::clone(&text);
        let parsing = cx.background_spawn(async move { parse_pair(&path, &parsed) });
        view.syntax.task = Some(cx.spawn(async move |this, cx| {
            let pair = parsing.await;
            let _ = this.update(cx, |window, cx| {
                window.install_syntax((tag, &text), pair, cx)
            });
        }));
    }

    /// Keeps `pair` on the open view if it still shows `text` of the screen `tag`, and redraws.
    fn install_syntax(
        &mut self,
        (tag, text): (u64, &Arc<PairText>),
        pair: PairSyntax,
        cx: &mut Context<Self>,
    ) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let view = file
            .filter(|file| file.tag == tag)
            .and_then(|file| file.view.as_mut())
            .filter(|view| Arc::ptr_eq(view.text(), text));
        if let Some(view) = view {
            view.syntax.ready = Some(Rc::new(pair));
            view.syntax.provisional = None;
            view.syntax.task = None;
            cx.notify();
        }
    }
}

#[cfg(test)]
impl NavoWindow {
    /// The open file view's parsed pair, once its background parse ended.
    pub(in crate::window_view) fn file_syntax(&self) -> Option<Rc<PairSyntax>> {
        self.syntax_slot()?.ready.clone()
    }

    /// The pair the open file view's rows colour with now: the finished parse, or the provisional
    /// one kept across a reload until it lands.
    pub(in crate::window_view) fn file_syntax_shown(&self, cx: &App) -> Option<Rc<PairSyntax>> {
        self.syntax_slot()?.parts(cx).0
    }

    /// The parse the open file view keeps from before a reload, until its own one lands.
    pub(in crate::window_view) fn file_syntax_provisional(&self) -> Option<Rc<PairSyntax>> {
        self.syntax_slot()?.provisional.clone()
    }

    /// The syntax slot of the open file view, or `None` with no text pair open.
    fn syntax_slot(&self) -> Option<&SyntaxSlot> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        Some(&file.view.as_ref()?.syntax)
    }
}
