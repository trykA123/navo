use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

use navo_clipboard::{
    ClipSink, NoSealer, Ports, SealError, Sealer, SecretBytes, Sequence, SinkError, SystemClock,
};
use navo_shell::{CaptureContent, ShellCapture};
use navo_ui::clip_palette::{ClipMode, ClipboardHooks};

use super::{Backends, ClipModule, Config, OnCapture, StartWatch};

mod follow;
mod history;
mod intro;
mod lifecycle;
mod saved;

pub(super) type Slot = Arc<Mutex<Option<OnCapture>>>;

struct Guard(Slot);

impl Drop for Guard {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}

/// Texts that start with this take a while to write.
pub(super) const SLOW: &str = "slow:";
/// Writing a text with this prefix panics, as a sink that crashes the worker would.
pub(super) const PANIC: &str = "panic:";

#[derive(Default)]
pub(super) struct FakeSink {
    pub(super) texts: Mutex<Vec<String>>,
    pub(super) privates: Mutex<Vec<String>>,
}

impl ClipSink for FakeSink {
    fn write_text(&self, text: &str) -> Result<(), SinkError> {
        assert!(!text.starts_with(PANIC), "the sink was told to panic");
        if text.starts_with(SLOW) {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        self.texts.lock().unwrap().push(text.to_owned());
        Ok(())
    }

    fn write_files(&self, _paths: &[PathBuf], _cut: bool) -> Result<(), SinkError> {
        Err(SinkError::Unsupported)
    }

    fn write_private(&self, text: &str) -> Result<Sequence, SinkError> {
        self.privates.lock().unwrap().push(text.to_owned());
        Ok(Sequence(1))
    }

    fn clear_if_sequence(&self, _sequence: Sequence) -> Result<bool, SinkError> {
        Ok(true)
    }
}

/// Seals by flipping bits, so the sealed bytes never hold the plain text.
pub(super) struct XorSealer;

impl Sealer for XorSealer {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError> {
        Ok(plain.iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unseal(&self, sealed: &[u8]) -> Result<SecretBytes, SealError> {
        Ok(SecretBytes::new(
            sealed.iter().map(|byte| byte ^ 0x5a).collect(),
        ))
    }
}

pub(super) struct Rig {
    pub(super) module: Arc<ClipModule>,
    pub(super) watcher: Slot,
    pub(super) sink: Arc<FakeSink>,
    pub(super) data: PathBuf,
    _dir: tempfile::TempDir,
}

impl Rig {
    pub(super) fn new() -> Self {
        Self::with_watch(None)
    }

    pub(super) fn with_watch(watch: Option<StartWatch>) -> Self {
        Self::build(watch, Arc::new(NoSealer), false)
    }

    pub(super) fn with_sealer(sealer: Arc<dyn Sealer>) -> Self {
        Self::build(None, sealer, true)
    }

    fn build(watch: Option<StartWatch>, sealer: Arc<dyn Sealer>, secrets: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let watcher = Slot::default();
        let sink = Arc::new(FakeSink::default());
        let slot = watcher.clone();
        let watch = watch.unwrap_or_else(|| {
            Arc::new(move |on: OnCapture| {
                *slot.lock().unwrap() = Some(on);
                Ok(Box::new(Guard(slot.clone())) as Box<dyn Send>)
            })
        });
        let ports = Ports {
            sink: sink.clone(),
            sealer,
            clock: Arc::new(SystemClock),
        };
        let module = Arc::new(ClipModule::new(
            Some(data.clone()),
            Backends {
                watch,
                ports,
                secrets,
            },
        ));
        Self {
            module,
            watcher,
            sink,
            data,
            _dir: dir,
        }
    }

    pub(super) fn apply(&self, config: Config) {
        self.module.apply_config(config);
        self.module.settle();
    }

    pub(super) fn db(&self) -> PathBuf {
        self.data.join("clipboard.db")
    }

    pub(super) fn emit(&self, text: &str) -> bool {
        self.emit_content(CaptureContent::Text(text.to_owned()))
    }

    pub(super) fn emit_content(&self, content: CaptureContent) -> bool {
        let guard = self.watcher.lock().unwrap();
        let Some(on) = guard.as_ref() else {
            return false;
        };
        on(ShellCapture {
            content,
            owner_exe: Some("editor".to_owned()),
            sequence: 1,
        });
        true
    }

    pub(super) fn titles(&self, query: &str) -> Vec<String> {
        let results = self.module.search(query, ClipMode::All, None);
        results.rows.into_iter().map(|row| row.title).collect()
    }

    pub(super) fn wait_for_title(&self, title: &str) {
        for _ in 0..1000 {
            if self.titles("").iter().any(|found| found == title) {
                return;
            }
            sleep(Duration::from_millis(5));
        }
        panic!("the row never reached the snapshot");
    }
}

pub(super) fn on() -> Config {
    Config {
        enabled: true,
        record: true,
    }
}

pub(super) fn off() -> Config {
    Config {
        enabled: false,
        record: true,
    }
}

pub(super) fn wait_until_len(rig: &Rig, len: usize) {
    for _ in 0..1000 {
        if rig.titles("").len() == len {
            return;
        }
        sleep(Duration::from_millis(5));
    }
    panic!("the rows never reached {len}");
}
