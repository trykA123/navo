use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::content::{self, CheckOptions, Side};
use crate::model::{
    CancelToken, CaseRule, ContentClass, EntryKind, Generation, Reason, RelPath, Row, SideEntry,
    SideSource, Status,
};
use crate::pair::pair;
use crate::rules::IgnoreRules;

use super::SessionOptions;
use super::classify::classify;
use super::shared::Shared;

pub(super) const MAX_WORKERS: usize = 4;
pub(super) const JOB_QUEUE: usize = 64;

pub(super) struct Run {
    pub(super) left: Arc<dyn SideSource>,
    pub(super) right: Arc<dyn SideSource>,
    pub(super) options: SessionOptions,
    pub(super) generation: Generation,
    pub(super) cancel: CancelToken,
    pub(super) shared: Arc<Shared>,
}

struct Context {
    left: Arc<dyn SideSource>,
    right: Arc<dyn SideSource>,
    options: SessionOptions,
    generation: Generation,
    cancel: CancelToken,
    shared: Arc<Shared>,
}

struct FileJob {
    id: u64,
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
    let context = Arc::new(Context {
        left: run.left,
        right: run.right,
        options: run.options,
        generation: run.generation,
        cancel: run.cancel,
        shared: run.shared,
    });
    let (job_sender, job_receiver) = mpsc::sync_channel(JOB_QUEUE);
    let job_receiver = Arc::new(Mutex::new(job_receiver));
    let mut handles = Vec::with_capacity(MAX_WORKERS + 1);
    for _ in 0..MAX_WORKERS {
        let context = Arc::clone(&context);
        let jobs = Arc::clone(&job_receiver);
        handles.push(thread::spawn(move || worker(&context, &jobs)));
    }
    let walker = Arc::clone(&context);
    handles.push(thread::spawn(move || {
        let inner = Arc::clone(&walker);
        let walked = catch_unwind(AssertUnwindSafe(move || walk(&inner, job_sender)));
        if walked.is_err() {
            walker.cancel.cancel();
            walker
                .shared
                .finish_problem(walker.generation, Reason::WorkerPanicked);
        }
    }));
    handles
}

fn worker(context: &Context, jobs: &Mutex<Receiver<FileJob>>) {
    loop {
        if context.cancel.is_cancelled() {
            break;
        }
        let job = {
            let receiver = jobs.lock().unwrap_or_else(PoisonError::into_inner);
            receiver.recv()
        };
        let Ok(job) = job else {
            break;
        };
        let Some(row) = context.shared.take_row(job.id) else {
            break;
        };
        if context.cancel.is_cancelled() {
            break;
        }
        let checked = catch_unwind(AssertUnwindSafe(|| check_row(context, row.clone())));
        let row = match checked {
            Ok(row) => row,
            Err(_) => {
                let mut row = row;
                row.raw_status = Status::Unavailable(Reason::WorkerPanicked);
                row.display_status = row.raw_status;
                row.content = ContentClass::Unknown;
                row
            }
        };
        if !context.shared.emit_final(context.generation, row) {
            break;
        }
    }
}

fn check_row(context: &Context, mut row: Row) -> Row {
    let (Some(left_entry), Some(right_entry)) = (row.left.as_deref(), row.right.as_deref()) else {
        return row;
    };
    let options = CheckOptions {
        cache: &*context.options.cache,
        cancel: &context.cancel,
        read_hint: context.options.read_hint,
        known_same: false,
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
        &mut |_| {},
    );
    row.raw_status = result.status;
    row.display_status = result.status;
    row.content = result.content;
    row
}

fn walk(context: &Context, jobs: SyncSender<FileJob>) {
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
        if !walk_folder(context, task, &mut tasks, &jobs) {
            break;
        }
    }
    context.shared.finish(context.generation);
}

fn walk_folder(
    context: &Context,
    task: WalkTask,
    tasks: &mut VecDeque<WalkTask>,
    jobs: &SyncSender<FileJob>,
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
    let ids = context.shared.store_rows(checking);
    if !context.shared.emit_initial(context.generation, rows) {
        return false;
    }
    for id in ids {
        if !send_job(jobs, FileJob { id }, &context.cancel) {
            return false;
        }
    }
    true
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
        row.generation = context.generation;
        classify(row, context.options.check_mode);
        push_folders(tasks, row, &context.options.rules);
        if row.raw_status == Status::Checking {
            checking.push(row.clone());
        }
    }
    Some(checking)
}

fn send_job(jobs: &SyncSender<FileJob>, mut job: FileJob, cancel: &CancelToken) -> bool {
    loop {
        if cancel.is_cancelled() {
            return false;
        }
        match jobs.try_send(job) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                job = returned;
                thread::sleep(Duration::from_millis(1));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
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
        let row = folder_error(context.generation, folder, side, reason);
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
    match (left, right) {
        (Some(left), Some(right)) => {
            if rules.enters(&left.rel) {
                tasks.push_back(WalkTask {
                    left: Some(left.rel.clone()),
                    right: Some(right.rel.clone()),
                    row: Some(row.clone()),
                });
            }
        }
        (Some(left), None) => {
            if rules.enters(&left.rel) {
                tasks.push_back(WalkTask {
                    left: Some(left.rel.clone()),
                    right: None,
                    row: Some(row.clone()),
                });
            }
        }
        (None, Some(right)) => {
            if rules.enters(&right.rel) {
                tasks.push_back(WalkTask {
                    left: None,
                    right: Some(right.rel.clone()),
                    row: Some(row.clone()),
                });
            }
        }
        (None, None) => {}
    }
}
