//! The Settings Undo stack: the old value of each change, newest last, and what the save
//! learned about it (was the key in navo.toml before?), so Undo can restore or remove it.

use crate::settings::{Change, Field, Value};
use crate::window_view::SaveReport;

/// What the save has told about the key of a step's field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Known {
    /// The change waits to be saved.
    Queued,
    /// The save is under way.
    Sent,
    /// The key was not in navo.toml before: Undo removes it.
    Absent,
    /// The key was in navo.toml: Undo writes the old value back.
    Present,
}

/// One change: the value it replaced and what the save learned.
struct Step {
    old: Change,
    known: Known,
}

/// The changes made since Settings opened, newest last.
#[derive(Default)]
pub(super) struct UndoStack {
    steps: Vec<Step>,
}

impl UndoStack {
    /// Records `old`, the value a change replaced.
    pub(super) fn push(&mut self, old: Change) {
        self.steps.push(Step {
            old,
            known: Known::Queued,
        });
    }

    /// True when there is nothing to undo.
    pub(super) fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Forgets every step.
    pub(super) fn clear(&mut self) {
        self.steps.clear();
    }

    /// The change that undoes the newest step: its old value, or `Unset` when the key was absent.
    pub(super) fn pop(&mut self) -> Option<Change> {
        let step = self.steps.pop()?;
        Some(match step.known {
            Known::Absent => Change {
                field: step.old.field,
                value: Value::Unset,
            },
            _ => step.old,
        })
    }

    /// Marks the queued steps of `fields` as sent to the save.
    pub(super) fn sent(&mut self, fields: &[Field]) {
        for step in &mut self.steps {
            if step.known == Known::Queued && fields.contains(&step.old.field) {
                step.known = Known::Sent;
            }
        }
    }

    /// Reads the save `report` for the `sent` fields and resolves their sent steps.
    pub(super) fn learned(&mut self, report: &SaveReport, sent: &[Field]) {
        for field in sent {
            let was_absent = !report.failed.contains(field) && report.was_absent.contains(field);
            self.resolve(*field, was_absent);
        }
    }

    /// The first sent step of `field` learns `was_absent`; later ones saw the key present.
    fn resolve(&mut self, field: Field, was_absent: bool) {
        let waiting = self
            .steps
            .iter_mut()
            .filter(|step| step.known == Known::Sent && step.old.field == field);
        for (index, step) in waiting.enumerate() {
            let is_first_absent = index == 0 && was_absent;
            step.known = if is_first_absent {
                Known::Absent
            } else {
                Known::Present
            };
        }
    }
}
