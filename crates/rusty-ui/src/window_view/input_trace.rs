use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use gpui_kit::{App, Global, Window, WindowId};
use rusty_fs::ProviderOps;
use serde::{Deserialize, Serialize};

mod frame;
mod provider;
#[cfg(test)]
mod tests;

pub(super) use frame::frame;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    action: String,
    key_to_frame_ms: f64,
    ui_busy_ms: f64,
    disk_calls: u32,
}

struct Input {
    action: String,
    started: Instant,
    dispatch: Duration,
    frame: Option<Instant>,
    disk_calls: u32,
}

type PendingInput = Rc<RefCell<Input>>;

#[derive(Clone)]
struct State {
    sender: Sender<Option<Record>>,
    calls: Arc<AtomicU64>,
    inputs: Rc<RefCell<HashMap<WindowId, PendingInput>>>,
    frames: Rc<RefCell<HashMap<WindowId, Vec<PendingInput>>>>,
}

impl Global for State {}

pub struct InputTrace {
    state: State,
    writer: Option<JoinHandle<()>>,
}

impl InputTrace {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let (sender, receiver) = mpsc::channel::<Option<Record>>();
        let writer = thread::Builder::new()
            .name("input-trace".into())
            .spawn(move || write_records(file, receiver))?;
        Ok(Self {
            state: State {
                sender,
                calls: Arc::new(AtomicU64::new(0)),
                inputs: Rc::default(),
                frames: Rc::default(),
            },
            writer: Some(writer),
        })
    }

    pub fn provider(&self, inner: Arc<dyn ProviderOps>) -> Arc<dyn ProviderOps> {
        Arc::new(provider::CountingFs::new(inner, self.state.calls.clone()))
    }

    pub(super) fn hooks(trace: Option<&Self>) -> impl FnOnce(&mut App) + use<> {
        let state = trace.map(|trace| trace.state.clone());
        move |cx| {
            let Some(state) = state else { return };
            cx.set_global(state.clone());
            cx.intercept_keystrokes(move |event, window, cx| {
                state.begin(format!("key:{}", event.keystroke), window, cx);
            })
            .detach();
            cx.observe_keystrokes(|event, window, cx| {
                cx.global::<State>().name_action(event, window);
            })
            .detach();
        }
    }
}

impl State {
    fn name_action(&self, event: &gpui_kit::KeystrokeEvent, window: &Window) {
        if let Some(action) = event.action.as_ref()
            && let Some(input) = self
                .inputs
                .borrow()
                .get(&window.window_handle().window_id())
        {
            input.borrow_mut().action = action.name().to_owned();
        }
    }

    fn begin(&self, action: String, window: &mut Window, cx: &mut App) {
        let input = Rc::new(RefCell::new(Input {
            action,
            started: cx.background_executor().now(),
            dispatch: Duration::ZERO,
            frame: None,
            disk_calls: 0,
        }));
        let id = window.window_handle().window_id();
        self.inputs.borrow_mut().insert(id, input.clone());
        let disk_before = self.calls.load(Ordering::Relaxed);
        let calls = self.calls.clone();
        let dispatched = input.clone();
        let inputs = self.inputs.clone();
        cx.defer(move |cx| {
            cx.defer(move |cx| {
                inputs.borrow_mut().remove(&id);
                let mut input = dispatched.borrow_mut();
                input.dispatch = cx.background_executor().now() - input.started;
                let count = calls.load(Ordering::Relaxed).saturating_sub(disk_before);
                input.disk_calls = u32::try_from(count).unwrap_or(u32::MAX);
            });
        });
        let frames = self.frames.clone();
        window.on_next_frame(move |window, cx| {
            input.borrow_mut().frame = Some(cx.background_executor().now());
            frames.borrow_mut().entry(id).or_default().push(input);
            window.refresh();
        });
    }

    fn finish(&self, id: WindowId, render: Duration) {
        let inputs = self.frames.borrow_mut().remove(&id).unwrap_or_default();
        for input in inputs {
            let input = input.borrow();
            let Some(frame) = input.frame else { continue };
            let record = Record {
                action: input.action.clone(),
                key_to_frame_ms: (frame - input.started).as_secs_f64() * 1000.,
                ui_busy_ms: (input.dispatch + render).as_secs_f64() * 1000.,
                disk_calls: input.disk_calls,
            };
            if let Err(error) = self.sender.send(Some(record)) {
                tracing::error!(%error, "input trace record could not be queued");
            }
        }
    }
}

impl Drop for InputTrace {
    fn drop(&mut self) {
        if self.state.sender.send(None).is_err() {
            tracing::error!("input trace writer stopped before shutdown");
        }
        if let Some(writer) = self.writer.take()
            && writer.join().is_err()
        {
            tracing::error!("input trace writer panicked");
        }
    }
}

fn write_record(writer: &mut impl Write, record: &Record) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, record)?;
    writer.write_all(b"\n")
}

fn write_records(file: File, receiver: Receiver<Option<Record>>) {
    let mut file = BufWriter::new(file);
    for record in receiver {
        let Some(record) = record else { break };
        if let Err(error) = write_record(&mut file, &record) {
            tracing::error!(%error, "cannot write input trace");
            break;
        }
    }
    if let Err(error) = file.flush() {
        tracing::error!(%error, "cannot flush input trace");
    }
}
