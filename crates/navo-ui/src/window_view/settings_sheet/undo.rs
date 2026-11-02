use crate::settings::{Change, Field, Value};
use crate::window_view::SaveReport;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Known {
    Queued,
    Sent,
    Absent,
    Present,
}

struct Step {
    old: Change,
    known: Known,
}

#[derive(Default)]
pub(super) struct UndoStack {
    steps: Vec<Step>,
}

impl UndoStack {
    pub(super) fn push(&mut self, old: Change) {
        self.steps.push(Step {
            old,
            known: Known::Queued,
        });
    }

    pub(super) fn clear(&mut self) {
        self.steps.clear();
    }

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

    pub(super) fn sent(&mut self, fields: &[Field]) {
        for step in &mut self.steps {
            if step.known == Known::Queued && fields.contains(&step.old.field) {
                step.known = Known::Sent;
            }
        }
    }

    pub(super) fn learned(&mut self, report: &SaveReport, sent: &[Field]) {
        for field in sent {
            let was_absent = !report.failed.contains(field) && report.was_absent.contains(field);
            self.resolve(*field, was_absent);
        }
    }

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
