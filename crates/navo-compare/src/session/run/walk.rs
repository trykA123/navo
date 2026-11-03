use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::Ordering;

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

pub(super) fn walk(context: &Context) {
    let mut tasks = VecDeque::new();
    tasks.push_back(WalkTask {
        left: Some(RelPath::root()),
        right: Some(RelPath::root()),
        row: None,
    });
    let network = context.left.is_network() || context.right.is_network();
    let mut meter = Meter::new(context.options.network_cap.filter(|_| network));
    while !tasks.is_empty() {
        if context.cancel.is_cancelled() || context.shared.is_closed() {
            break;
        }
        if let Some(stop) = meter.hit() {
            if !wait_for_continue(context, stop) {
                break;
            }
            meter.restart();
        }
        let Some(task) = tasks.pop_front() else {
            break;
        };
        if !walk_folder(context, task, &mut tasks, &mut meter) {
            break;
        }
    }
    context.shared.finish(context.generation);
}

/// Reports the stop and blocks until `continue_walk`; false when the walk must end.
fn wait_for_continue(context: &Context, stop: LimitStop) -> bool {
    context.shared.stop_at_limit(context.generation, stop)
        && context
            .shared
            .wait_resumed(context.generation, || context.cancel.is_cancelled())
}

fn walk_folder(
    context: &Context,
    task: WalkTask,
    tasks: &mut VecDeque<WalkTask>,
    meter: &mut Meter,
) -> bool {
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
    meter.add_folder(left_entries.len().saturating_add(right_entries.len()));
    let case = CaseRule::for_sides(
        context.left.case_sensitive(),
        context.right.case_sensitive(),
    );
    let mut rows = pair(left_entries, right_entries, case);
    if context.cancel.is_cancelled() {
        return false;
    }
    let Some(checking) = classify_rows(context, &mut rows, tasks) else {
        return false;
    };
    publish_checking(context, rows, checking, task.row.as_ref().map(|row| row.id))
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
        classify(row, context.options.check_mode);
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
    list_side(source, folder, &context.options.rules, &context.cancel)
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
