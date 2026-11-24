//! Lint problems in the F3 editor (E5): JSON, TOML, YAML, XML and CSV text is linted off the UI
//! thread after a 200 ms pause, shown as widget diagnostics, reached with F8 and counted in the header.

use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::input::{Diagnostic as Marked, DiagnosticSeverity, Position};
use gpui_kit::{Role, Stateful, Task};
use navo_core::Location;
use navo_l10n::t;
use navo_structured::lint::{Diagnostic, LintKind, Severity, lint};

use super::EditorView;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::keys::{NextProblem, PreviousProblem};
use crate::window_view::viewer::kinds::extension;

/// Quiet time after the last edit before the text is linted again.
const LINT_AFTER: Duration = Duration::from_millis(200);

/// The lint state of one editor: the file's lint kind, the last counts and the pending run.
#[derive(Default)]
pub(super) struct Lint {
    kind: Option<LintKind>,
    errors: usize,
    warnings: usize,
    task: Option<Task<()>>,
}

impl Lint {
    /// The lint state for the file at `location`; no kind (and no linting) for other extensions.
    pub(super) fn for_location(location: &Location) -> Self {
        let name = location
            .as_str()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default();
        let kind = match extension(name).as_deref() {
            Some("json") => Some(LintKind::Json),
            Some("toml") => Some(LintKind::Toml),
            Some("yaml" | "yml") => Some(LintKind::Yaml),
            Some("xml") => Some(LintKind::Xml),
            Some("csv") => Some(LintKind::Csv),
            _ => None,
        };
        Self {
            kind,
            ..Self::default()
        }
    }
}

/// 0-based line and character column of byte `offset` in `text`, as the editor widget counts them
/// (lines end at `\n`; the offset is clamped to the text and moved back to a char boundary).
fn position(text: &str, offset: usize) -> Position {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let column = before[line_start..].chars().count();
    Position::new(
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(column).unwrap_or(u32::MAX),
    )
}

/// Converts lint problems over `text` into widget diagnostics plus (errors, warnings) counts.
fn marked(text: &str, found: Vec<Diagnostic>) -> (Vec<Marked>, (usize, usize)) {
    let errors = found
        .iter()
        .filter(|problem| problem.severity == Severity::Error)
        .count();
    let warnings = found.len() - errors;
    let marks = found
        .into_iter()
        .map(|problem| {
            let severity = match problem.severity {
                Severity::Error => DiagnosticSeverity::Error,
                Severity::Warning => DiagnosticSeverity::Warning,
            };
            let range = position(text, problem.range.start)..position(text, problem.range.end);
            Marked::new(range, problem.message).with_severity(severity)
        })
        .collect();
    (marks, (errors, warnings))
}

impl EditorView {
    /// Lints the text after `delay` on the background executor; a newer call cancels this run.
    /// Does nothing for files without a lint kind.
    pub(super) fn lint_later(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(kind) = self.lint.kind else {
            return;
        };
        self.lint.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let Ok((text, revision)) = this.update(cx, |view, cx| {
                (view.editor.read(cx).text().clone(), view.revision)
            }) else {
                return;
            };
            let found = cx
                .background_spawn(async move {
                    let text = text.to_string();
                    let found = lint(kind, &text);
                    marked(&text, found)
                })
                .await;
            let _ = this.update(cx, |view, cx| view.show_lint(revision, found, cx));
        }));
    }

    /// Lints again after the quiet time; called on every text change.
    pub(super) fn lint_edited(&mut self, cx: &mut Context<Self>) {
        self.lint_later(LINT_AFTER, cx);
    }

    /// Writes a lint result into the widget unless the text changed since `revision` (stale run).
    fn show_lint(
        &mut self,
        revision: u64,
        (marks, (errors, warnings)): (Vec<Marked>, (usize, usize)),
        cx: &mut Context<Self>,
    ) {
        if revision != self.revision {
            return;
        }
        (self.lint.errors, self.lint.warnings) = (errors, warnings);
        self.editor.update(cx, |state, cx| {
            let text = state.text().clone();
            if let Some(set) = state.diagnostics_mut() {
                set.reset(&text);
                set.extend(marks);
            }
            cx.notify();
        });
        cx.notify();
    }

    /// Moves the cursor to the start of the next (or previous) diagnostic, wrapping at the ends.
    fn jump_to_problem(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| {
            let cursor = state.cursor();
            let starts: Vec<(usize, Position)> = state
                .diagnostics()
                .map(|set| {
                    set.iter()
                        .map(|entry| (entry.range.start, entry.diagnostic.range.start))
                        .collect()
                })
                .unwrap_or_default();
            let target = if forward {
                starts
                    .iter()
                    .find(|(at, _)| *at > cursor)
                    .or(starts.first())
            } else {
                starts
                    .iter()
                    .rev()
                    .find(|(at, _)| *at < cursor)
                    .or(starts.last())
            };
            if let Some((_, start)) = target {
                state.set_cursor_position(*start, window, cx);
            }
        });
    }

    /// Adds the F8 / Shift+F8 problem jumps to the editor root.
    pub(super) fn lint_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &NextProblem, window, cx| {
            view.jump_to_problem(true, window, cx);
        }))
        .on_action(cx.listener(|view, _: &PreviousProblem, window, cx| {
            view.jump_to_problem(false, window, cx);
        }))
    }

    /// The header's "1 error · 2 warnings" count; `None` when the last lint found nothing.
    pub(super) fn lint_status(&self, t: &Theme) -> Option<AnyElement> {
        let mut parts = Vec::new();
        if self.lint.errors > 0 {
            parts.push((
                t!("viewer-lint-errors", count = self.lint.errors),
                t.colors.mark,
            ));
        }
        if self.lint.warnings > 0 {
            parts.push((
                t!("viewer-lint-warnings", count = self.lint.warnings),
                t.colors.warn,
            ));
        }
        if parts.is_empty() {
            return None;
        }
        let label: Vec<&str> = parts.iter().map(|(text, _)| text.as_str()).collect();
        let mut row = div()
            .id("lint-count")
            .flex()
            .flex_none()
            .gap(t.spacing.xs)
            .role(Role::Status)
            .aria_label(label.join(" · "))
            .test_support()
            .text_size(t.typography.small);
        for (index, (text, colour)) in parts.into_iter().enumerate() {
            if index > 0 {
                row = row.child(div().text_color(t.colors.muted).child("·"));
            }
            row = row.child(div().text_color(colour).child(text));
        }
        Some(row.into_any_element())
    }
}
