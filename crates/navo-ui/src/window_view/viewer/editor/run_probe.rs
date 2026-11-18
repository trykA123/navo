//! Test-only recorder for the colour runs the editor asks its line highlighter for.
//! GPUI test snapshots do not expose editor runs, so the tests read them here.
use std::cell::RefCell;
use std::ops::Range;

use gpui_kit::base::input::EditorState;
use gpui_kit::base::input::{
    FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, InputHighlighterFactory, Rope,
};
use gpui_kit::{Context, HighlightStyle, SharedString, Window};

/// One coloured stretch the editor asked for: its text and style.
pub type Run = (String, HighlightStyle);

thread_local! {
    static RUNS: RefCell<Vec<Run>> = const { RefCell::new(Vec::new()) };
}

/// Drains and returns the recorded runs, oldest first.
pub fn take() -> Vec<Run> {
    RUNS.with_borrow_mut(std::mem::take)
}

/// Wraps `inner` so every `styles` call is recorded with the text it covered.
struct Recording {
    inner: Box<dyn InputHighlighter>,
    text: Rope,
}

impl InputHighlighter for Recording {
    fn language(&self) -> SharedString {
        self.inner.language()
    }

    fn update(
        &mut self,
        edit: Option<InputEdit>,
        text: &Rope,
        folding: bool,
        window: &mut Window,
        cx: &mut Context<EditorState>,
    ) {
        self.text = text.clone();
        self.inner.update(edit, text, folding, window, cx);
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let runs = self.inner.styles(range, resolver);
        let whole = self.text.to_string();
        let recorded = runs
            .iter()
            .filter_map(|(run, style)| Some((whole.get(run.clone())?.to_string(), *style)));
        RUNS.with_borrow_mut(|out| out.extend(recorded));
        runs
    }

    fn fold_ranges(&self, text: &Rope) -> Vec<FoldRange> {
        self.inner.fold_ranges(text)
    }
}

/// Wraps `factory` so the highlighters it builds record their runs.
pub fn recording(factory: InputHighlighterFactory) -> InputHighlighterFactory {
    std::rc::Rc::new(move |language: &str| {
        factory(language).map(|inner| {
            let wrapped: Box<dyn InputHighlighter> = Box::new(Recording {
                inner,
                text: Rope::new(),
            });
            wrapped
        })
    })
}
