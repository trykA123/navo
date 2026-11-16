//! Walks compare folders, classifies rows and queues content checks.

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::model::{
    CancelToken, CaseRule, ContentClass, EntryKind, Generation, Reason, RelPath, Row, SideEntry,
    SideSource, Status,
};
use crate::pair::pair;
use crate::queue::JobSpec;
use crate::rules::IgnoreRules;

use super::Context;
use crate::session::classify::classify;
use crate::session::limit::{LimitStop, Meter};
use crate::session::{ComparePhase, PhaseTelemetry};

#[derive(Clone)]
struct WalkTask {
    left: Option<RelPath>,
    right: Option<RelPath>,
    row: Option<Row>,
}

#[derive(Clone, Copy)]
enum SideId {
    Left,
    Right,
}

#[derive(Default)]
struct WalkProgress {
    listed: u64,
    quick_checked: u64,
}

struct WalkState {
    tasks: VecDeque<WalkTask>,
    meter: Meter,
    progress: WalkProgress,
}

pub(super) fn walk(context: &Context) {
    let network = context.left.is_network() || context.right.is_network();
    let mut state = WalkState {
        tasks: VecDeque::new(),
        meter: Meter::new(context.options.network_cap.filter(|_| network)),
        progress: WalkProgress::default(),
    };
    state.tasks.push_back(WalkTask {
        left: Some(RelPath::root()),
        right: Some(RelPath::root()),
        row: None,
    });
    while !state.tasks.is_empty() {
        if context.cancel.is_cancelled() || context.shared.is_closed() {
            break;
        }
        if let Some(stop) = state.meter.hit() {
            if !wait_for_continue(context, stop) {
                break;
            }
            state.meter.restart();
        }
        let Some(task) = state.tasks.pop_front() else {
            break;
        };
        if !walk_folder(context, task, &mut state) {
            break;
        }
    }
    record_quick_check(
        context,
        state.progress.quick_checked,
        Some(state.progress.quick_checked),
    );
    context.shared.finish(context.generation);
}

/// Reports the stop and blocks until `continue_walk`; false when the walk must end.
fn wait_for_continue(context: &Context, stop: LimitStop) -> bool {
    context.shared.stop_at_limit(context.generation, stop)
        && context
            .shared
            .wait_resumed(context.generation, || context.cancel.is_cancelled())
}

fn walk_folder(context: &Context, task: WalkTask, state: &mut WalkState) -> bool {
    if context.cancel.is_cancelled() {
        return false;
    }
    let left_entries = match list_task(context, task.left.as_ref(), SideId::Left) {
        Ok(entries) => entries,
        Err(reason) => return fail_folder(context, &task, SideId::Left, reason),
    };
    if context.cancel.is_cancelled() {
        return false;
    }
    let right_entries = match list_task(context, task.right.as_ref(), SideId::Right) {
        Ok(entries) => entries,
        Err(reason) => return fail_folder(context, &task, SideId::Right, reason),
    };
    if context.cancel.is_cancelled() {
        return false;
    }
    let entry_count = left_entries.len().saturating_add(right_entries.len());
    state.meter.add_folder(entry_count);
    state.progress.listed = state
        .progress
        .listed
        .saturating_add(u64::try_from(entry_count).unwrap_or(u64::MAX));
    record_listing(context, state.progress.listed);
    let case = CaseRule::for_sides(
        context.left.case_sensitive(),
        context.right.case_sensitive(),
    );
    let mut rows = pair(left_entries, right_entries, case);
    if context.cancel.is_cancelled() {
        return false;
    }
    let Some(checking) = classify_rows(context, &mut rows, &mut state.tasks) else {
        return false;
    };
    let row_count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    let emitted = publish_checking(context, rows, checking, task.row.as_ref().map(|row| row.id));
    if emitted {
        state.progress.quick_checked = state.progress.quick_checked.saturating_add(row_count);
        record_quick_check(context, state.progress.quick_checked, None);
    }
    emitted
}

fn record_listing(context: &Context, done: u64) {
    context.shared.record_phase(
        context.generation,
        PhaseTelemetry {
            phase: ComparePhase::Listing,
            done,
            total: None,
        },
    );
}

fn record_quick_check(context: &Context, done: u64, total: Option<u64>) {
    context.shared.record_phase(
        context.generation,
        PhaseTelemetry {
            phase: ComparePhase::QuickCheck,
            done,
            total,
        },
    );
}

fn publish_checking(
    context: &Context,
    rows: Vec<Row>,
    checking: Vec<Row>,
    parent: Option<u64>,
) -> bool {
    let jobs: Vec<JobSpec> = checking.iter().map(|row| job_spec(context, row)).collect();
    context.queue.pause();
    context.shared.store_rows(checking, parent);
    context.queue.push_batch(&jobs);
    let emitted = context
        .shared
        .emit_initial(context.generation, rows, parent);
    context.queue.resume();
    emitted
}

fn job_spec(context: &Context, row: &Row) -> JobSpec {
    let left = row.left.as_deref();
    let right = row.right.as_deref();
    let size = left
        .map_or(0, |entry| entry.size)
        .max(right.map_or(0, |entry| entry.size));
    let volumes = (
        left.map_or(0, |entry| context.left.volume(&entry.rel)),
        right.map_or(0, |entry| context.right.volume(&entry.rel)),
    );
    JobSpec {
        id: row.id,
        size,
        volumes,
    }
}

/// Classifies listed rows with their sources and collects those needing content checks.
fn classify_rows(
    context: &Context,
    rows: &mut [Row],
    tasks: &mut VecDeque<WalkTask>,
) -> Option<Vec<Row>> {
    let mut checking = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        if index.is_multiple_of(1024) && context.cancel.is_cancelled() {
            return None;
        }
        row.id = context.next_row_id.fetch_add(1, Ordering::Relaxed);
        row.generation = context.generation;
        classify(
            row,
            context.options.check_mode,
            [context.left.as_ref(), context.right.as_ref()],
        );
        push_folders(tasks, row, &context.options.rules);
        if row.raw_status == Status::Checking {
            checking.push(row.clone());
        }
    }
    Some(checking)
}

fn fail_folder(context: &Context, task: &WalkTask, side: SideId, reason: Reason) -> bool {
    let Some(mut row) = task.row.clone() else {
        let folder = match side {
            SideId::Left => task.left.as_ref(),
            SideId::Right => task.right.as_ref(),
        };
        let Some(folder) = folder else {
            return true;
        };
        let mut row = folder_error(context.generation, folder, side, reason);
        row.id = context.next_row_id.fetch_add(1, Ordering::Relaxed);
        return context
            .shared
            .emit_initial(context.generation, vec![row], None);
    };
    let previous = row.raw_status;
    row.raw_status = Status::Unavailable(reason);
    row.display_status = row.raw_status;
    row.content = ContentClass::Unknown;
    context
        .shared
        .emit_update(context.generation, row, previous)
}

fn list_task(
    context: &Context,
    folder: Option<&RelPath>,
    side: SideId,
) -> Result<Vec<SideEntry>, Reason> {
    let Some(folder) = folder else {
        return Ok(Vec::new());
    };
    let source: &dyn SideSource = match side {
        SideId::Left => &*context.left,
        SideId::Right => &*context.right,
    };
    let started = Instant::now();
    let result = list_side(source, folder, &context.options.rules, &context.cancel);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    context
        .shared
        .record_slow(context.generation, folder.clone(), elapsed_ms);
    result
}

fn list_side(
    source: &dyn SideSource,
    folder: &RelPath,
    rules: &IgnoreRules,
    cancel: &CancelToken,
) -> Result<Vec<SideEntry>, Reason> {
    let mut entries = Vec::new();
    source.list(
        folder,
        &mut |mut entry| {
            if cancel.is_cancelled() {
                return ControlFlow::Break(());
            }
            let is_dir = entry.kind == EntryKind::Folder;
            if !rules.keeps(&entry.rel, is_dir) {
                return ControlFlow::Continue(());
            }
            entry.ignored = rules.is_ignored(&entry.rel, is_dir);
            entries.push(entry);
            ControlFlow::Continue(())
        },
        cancel,
    )?;
    Ok(entries)
}

fn folder_error(generation: Generation, folder: &RelPath, side: SideId, reason: Reason) -> Row {
    let entry = Arc::new(SideEntry::new(folder.clone(), EntryKind::Folder, 0, None));
    let (left, right) = match side {
        SideId::Left => (Some(entry), None),
        SideId::Right => (None, Some(entry)),
    };
    Row {
        id: 0,
        left,
        right,
        raw_status: Status::Unavailable(reason),
        display_status: Status::Unavailable(reason),
        generation,
        content: ContentClass::Unknown,
        contains_differences: false,
        differences_inside: 0,
    }
}

fn push_folders(tasks: &mut VecDeque<WalkTask>, row: &Row, rules: &IgnoreRules) {
    let left = row
        .left
        .as_deref()
        .filter(|entry| entry.kind == EntryKind::Folder);
    let right = row
        .right
        .as_deref()
        .filter(|entry| entry.kind == EntryKind::Folder);
    let (left, right) = match (left, right) {
        (Some(left), right) => (Some(left.rel.clone()), right.map(|entry| entry.rel.clone())),
        (None, Some(right)) => (None, Some(right.rel.clone())),
        (None, None) => return,
    };
    let Some(folder) = left.as_ref().or(right.as_ref()) else {
        return;
    };
    if rules.enters(folder) {
        tasks.push_back(WalkTask {
            left,
            right,
            row: Some(row.clone()),
        });
    }
}
