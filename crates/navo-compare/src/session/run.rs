use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};

use crate::content::{self, CheckOptions, ContentProgress, Side};
use crate::model::{
    CancelToken, CaseRule, ContentClass, EntryKind, Generation, Reason, RelPath, Row, SideEntry,
    SideSource, Status,
};
use crate::pair::pair;
use crate::queue::{CheckEvent, Job, JobSpec, Queue, Settlement};
use crate::rules::IgnoreRules;

use super::SessionOptions;
use super::classify::classify;
use super::shared::Shared;

#[cfg(test)]
pub(super) type SettleHook = Arc<dyn Fn(bool, u64) + Send + Sync>;

pub(super) struct Run {
    pub(super) left: Arc<dyn SideSource>,
    pub(super) right: Arc<dyn SideSource>,
    pub(super) options: SessionOptions,
    pub(super) generation: Generation,
    pub(super) cancel: CancelToken,
    pub(super) shared: Arc<Shared>,
    pub(super) queue: Arc<Queue>,
    pub(super) next_row_id: Arc<AtomicU64>,
    #[cfg(test)]
    pub(super) settle_hook: Option<SettleHook>,
}

struct Context {
    left: Arc<dyn SideSource>,
    right: Arc<dyn SideSource>,
    options: SessionOptions,
    generation: Generation,
    cancel: CancelToken,
    shared: Arc<Shared>,
    queue: Arc<Queue>,
    next_row_id: Arc<AtomicU64>,
    #[cfg(test)]
    settle_hook: Option<SettleHook>,
}

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

pub(super) fn spawn(run: Run) -> Vec<JoinHandle<()>> {
    let workers = run.queue.workers();
    let context = Arc::new(Context {
        left: run.left,
        right: run.right,
        options: run.options,
        generation: run.generation,
        cancel: run.cancel,
        shared: run.shared,
        queue: run.queue,
        next_row_id: run.next_row_id,
        #[cfg(test)]
        settle_hook: run.settle_hook,
    });
    let mut handles = Vec::with_capacity(workers + 1);
    for _ in 0..workers {
        let context = Arc::clone(&context);
        handles.push(thread::spawn(move || worker(&context)));
    }
    let walker = Arc::clone(&context);
    handles.push(thread::spawn(move || {
        let inner = Arc::clone(&walker);
        let walked = catch_unwind(AssertUnwindSafe(move || walk(&inner)));
        if walked.is_err() {
            walker.cancel.cancel();
            walker.queue.close();
            walker
                .shared
                .finish_problem(walker.generation, Reason::WorkerPanicked);
        }
    }));
    handles
}

#[cfg(test)]
fn call_settle_hook(context: &Context, after_complete: bool, id: u64) {
    if let Some(hook) = &context.settle_hook {
        hook(after_complete, id);
    }
}

fn worker(context: &Context) {
    loop {
        if context.cancel.is_cancelled() {
            break;
        }
        let Some(event) = context.queue.next() else {
            break;
        };
        match event {
            CheckEvent::Job(job) => {
                if !run_job(context, job) {
                    break;
                }
            }
            CheckEvent::Stopped(id) => {
                let Some(mut row) = context.shared.take_row(id) else {
                    continue;
                };
                row.raw_status = Status::NotChecked(Reason::Stopped);
                row.display_status = row.raw_status;
                row.content = ContentClass::Unknown;
                if !context.shared.emit_final(context.generation, row) {
                    break;
                }
            }
        }
    }
}

fn run_job(context: &Context, job: Job) -> bool {
    let Some(row) = context.shared.take_row(job.id) else {
        context.queue.complete(job.id);
        return false;
    };
    if context.cancel.is_cancelled() {
        context.queue.complete(job.id);
        return false;
    }
    let checked = catch_unwind(AssertUnwindSafe(|| check_row(context, row.clone(), &job)));
    let mut row = match checked {
        Ok(row) => row,
        Err(_) => {
            let mut row = row;
            row.raw_status = Status::Unavailable(Reason::WorkerPanicked);
            row.display_status = row.raw_status;
            row.content = ContentClass::Unknown;
            row
        }
    };
    #[cfg(test)]
    call_settle_hook(context, false, job.id);
    let settlement = context.queue.complete(job.id);
    #[cfg(test)]
    call_settle_hook(context, true, job.id);
    if settlement == Settlement::Stopped {
        row.raw_status = Status::NotChecked(Reason::Stopped);
        row.display_status = row.raw_status;
        row.content = ContentClass::Unknown;
    }
    context.shared.emit_final(context.generation, row)
}

fn check_row(context: &Context, mut row: Row, job: &Job) -> Row {
    let (Some(left_entry), Some(right_entry)) = (row.left.as_deref(), row.right.as_deref()) else {
        return row;
    };
    let options = CheckOptions {
        cache: &*context.options.cache,
        cancel: &job.cancel,
        read_hint: job.read_hint,
        known_same: false,
    };
    let mut report = |progress: ContentProgress| {
        if let Some(progress) =
            context
                .queue
                .progress(job.id, progress.bytes_done, progress.bytes_total)
        {
            context.shared.emit_progress(context.generation, progress);
        }
    };
    let result = content::check(
        Side {
            entry: left_entry,
            source: &*context.left,
        },
        Side {
            entry: right_entry,
            source: &*context.right,
        },
        &options,
        &mut report,
    );
    row.raw_status = result.status;
    row.display_status = result.status;
    row.content = result.content;
    row
}

fn walk(context: &Context) {
    let mut tasks = VecDeque::new();
    tasks.push_back(WalkTask {
        left: Some(RelPath::root()),
        right: Some(RelPath::root()),
        row: None,
    });
    while let Some(task) = tasks.pop_front() {
        if context.cancel.is_cancelled() || context.shared.is_closed() {
            break;
        }
        if !walk_folder(context, task, &mut tasks) {
            break;
        }
    }
    context.shared.finish(context.generation);
}

fn walk_folder(context: &Context, task: WalkTask, tasks: &mut VecDeque<WalkTask>) -> bool {
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
    publish_checking(context, rows, checking)
}

fn publish_checking(context: &Context, rows: Vec<Row>, checking: Vec<Row>) -> bool {
    let jobs: Vec<JobSpec> = checking.iter().map(|row| job_spec(context, row)).collect();
    context.queue.pause();
    context.shared.store_rows(checking);
    context.queue.push_batch(&jobs);
    let emitted = context.shared.emit_initial(context.generation, rows);
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
        return context.shared.emit_initial(context.generation, vec![row]);
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
