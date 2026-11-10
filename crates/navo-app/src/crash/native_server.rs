//! Out-of-process crash server: writes one native minidump into the app's `logs` folder and
//! records its redacted summary as a `crash-*.json`, so the crash notice picks it up.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::ExitCode;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::mpsc::{self, RecvTimeoutError};
#[cfg(windows)]
use std::sync::{Arc, Mutex, PoisonError};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use minidumper::{LoopAction, MinidumpBinary, Server, ServerHandler, SocketName};

use super::hook::{self, Record};
use super::native_summary::{record, summarize};

/// The dumps keep stacks and full module paths (RP3), so only the newest two stay on disk.
const KEEP_DUMPS: usize = 2;
const MAX_DUMP_BYTES: u64 = 64 * 1024 * 1024;
/// The server exits when no client connected within this time (RP2).
#[cfg(windows)]
const IDLE_EXIT: Duration = Duration::from_secs(10);

/// Owns the `logs` folder for native dumps: names, summarizes and prunes `crash-*.dmp` files.
pub(super) struct DumpSink {
    logs: PathBuf,
}

impl DumpSink {
    /// Creates the sink and the `logs` folder; fails when the folder cannot be created.
    pub(super) fn new(logs: &Path) -> io::Result<Self> {
        fs::create_dir_all(logs)?;
        Ok(Self {
            logs: logs.to_path_buf(),
        })
    }

    /// Opens a new `crash-<UTC stamp>-<pid>.dmp`; fails rather than overwrite an existing file.
    pub(super) fn create(&self) -> io::Result<(File, PathBuf)> {
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let path = self
            .logs
            .join(format!("crash-{stamp}-{}.dmp", std::process::id()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok((file, path))
    }

    /// Records the summary of `dump` (or an "unreadable" record) and keeps the newest dumps;
    /// returns the record path, and fails only when the record cannot be written.
    pub(super) fn finish(&self, dump: &Path) -> io::Result<PathBuf> {
        let summary = read_capped(dump).map(|bytes| summarize(&bytes));
        let record = match summary {
            Ok(Ok(crash)) => record(&crash),
            Ok(Err(error)) => {
                tracing::warn!(%error, "native crash dump did not parse");
                unreadable()
            }
            Err(error) => {
                tracing::warn!(kind = %error.kind(), "native crash dump could not be read");
                unreadable()
            }
        };
        let written = hook::write_record(&self.logs, &record)?;
        prune_dumps(&self.logs, dump);
        Ok(written)
    }
}

/// Reads at most `MAX_DUMP_BYTES` of `path`; fails when the file cannot be opened or read.
fn read_capped(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_DUMP_BYTES)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The record for a crash whose dump is missing or does not parse: no frames to show.
fn unreadable() -> Record {
    Record {
        version: env!("CARGO_PKG_VERSION"),
        commit: env!("NAVO_COMMIT"),
        thread: "native".into(),
        location: None,
        message: "native crash: unreadable minidump".into(),
        backtrace: String::new(),
        events: Vec::new(),
    }
}

/// Deletes all but the newest `KEEP_DUMPS` dump files in `logs`, never `current`. Links and
/// names that are not `crash-<date>-<time>-<pid>.dmp` are left alone; failures are logged.
fn prune_dumps(logs: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(logs) else {
        return;
    };
    let mut dumps: Vec<_> = entries
        .filter_map(Result::ok)
        // `DirEntry::file_type` does not follow links, so a junction or symlink is never a dump.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            dump_key(&entry.file_name().to_string_lossy()).map(|key| (key, entry.path()))
        })
        .collect();
    dumps.sort();
    let excess = dumps.len().saturating_sub(KEEP_DUMPS);
    for (_, old) in dumps.into_iter().take(excess) {
        if old == current {
            continue;
        }
        if let Err(error) = fs::remove_file(&old) {
            tracing::warn!(kind = %error.kind(), "could not prune an old native crash dump");
        }
    }
}

/// Sort key `(date, time, pid)` of a `crash-<8 digits>-<6 digits>-<pid>.dmp` name, or none.
fn dump_key(name: &str) -> Option<(u32, u32, u32)> {
    let stem = name.strip_prefix("crash-")?.strip_suffix(".dmp")?;
    let mut parts = stem.split('-');
    let (date, time, pid) = (parts.next()?, parts.next()?, parts.next()?);
    let digits = |part: &str, len: usize| {
        (part.len() == len && part.bytes().all(|b| b.is_ascii_digit())).then_some(())
    };
    digits(date, 8)?;
    digits(time, 6)?;
    if parts.next().is_some() || pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((date.parse().ok()?, time.parse().ok()?, pid.parse().ok()?))
}

/// Serves one native crash dump on `socket` into `logs`, then removes the socket file.
/// Returns 0 after a clean exit and 1 when the server could not start or failed.
#[cfg(windows)]
pub(crate) fn serve(socket: &Path, logs: &Path) -> ExitCode {
    let code = match run_server(socket, logs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::warn!(%error, "crash server stopped");
            ExitCode::from(1)
        }
    };
    match fs::remove_file(socket) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(kind = %error.kind(), "could not remove the crash socket"),
    }
    code
}

/// Binds `socket` and runs the message loop until one dump, the client's exit, or the
/// idle watchdog; fails when the folder, socket, watchdog thread or loop fails.
#[cfg(windows)]
fn run_server(socket: &Path, logs: &Path) -> Result<(), minidumper::Error> {
    let sink = DumpSink::new(logs)?;
    let mut server = Server::with_name(SocketName::Path(socket))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let connected = Arc::new(AtomicBool::new(false));
    // Dropping `stop` ends the watchdog early, so serve never waits the full idle time.
    let (stop, stopped) = mpsc::channel::<()>();
    let watchdog = {
        let (shutdown, connected) = (shutdown.clone(), connected.clone());
        std::thread::Builder::new()
            .name("crash-server-watchdog".into())
            .spawn(move || {
                let idle = stopped.recv_timeout(IDLE_EXIT) == Err(RecvTimeoutError::Timeout);
                if idle && !connected.load(Ordering::SeqCst) {
                    shutdown.store(true, Ordering::SeqCst);
                }
            })?
    };
    let handler = Handler {
        sink,
        connected,
        created: Mutex::new(None),
    };
    let result = server.run(Box::new(handler), &shutdown, None);
    drop(stop);
    if watchdog.join().is_err() {
        tracing::warn!("crash server watchdog panicked");
    }
    result
}

/// The server's callbacks: one dump per run, then exit.
#[cfg(windows)]
struct Handler {
    sink: DumpSink,
    connected: Arc<AtomicBool>,
    /// The dump file opened for the current request, so a failed write still gets a record.
    created: Mutex<Option<PathBuf>>,
}

#[cfg(windows)]
impl ServerHandler for Handler {
    fn create_minidump_file(&self) -> io::Result<(File, PathBuf)> {
        let (file, path) = self.sink.create()?;
        *self.created.lock().unwrap_or_else(PoisonError::into_inner) = Some(path.clone());
        Ok((file, path))
    }

    fn on_minidump_created(&self, result: Result<MinidumpBinary, minidumper::Error>) -> LoopAction {
        let dump = match result {
            Ok(binary) => {
                // The record must never point at a dump that a power loss could still drop.
                if let Err(error) = binary.file.sync_all() {
                    tracing::warn!(kind = %error.kind(), "could not flush the native crash dump");
                }
                Some(binary.path)
            }
            Err(error) => {
                tracing::warn!(%error, "native crash dump failed");
                self.created
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
            }
        };
        if let Some(dump) = dump
            && let Err(error) = self.sink.finish(&dump)
        {
            tracing::warn!(kind = %error.kind(), "could not record the native crash");
        }
        LoopAction::Exit
    }

    fn on_message(&self, _kind: u32, _buffer: Vec<u8>) {}

    fn on_client_connected(&self, _clients: usize) -> LoopAction {
        self.connected.store(true, Ordering::SeqCst);
        LoopAction::Continue
    }

    fn on_client_disconnected(&self, clients: usize) -> LoopAction {
        if clients == 0 {
            LoopAction::Exit
        } else {
            LoopAction::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use minidump_synth::{Exception, SynthMinidump, SystemInfo};
    use serde_json::Value;
    use test_assembler::Endian;

    use super::*;

    /// Builds a minimal Windows AMD64 access violation dump with no modules.
    fn access_violation() -> Vec<u8> {
        let endian = Endian::Little;
        let mut exception = Exception::new(endian);
        exception.exception_record.exception_code = 0xc000_0005;
        exception.exception_record.exception_address = 0x1234;
        SynthMinidump::with_endian(endian)
            .add_system_info(
                SystemInfo::new(endian)
                    .set_processor_architecture(9)
                    .set_platform_id(2),
            )
            .add_exception(exception)
            .finish()
            .expect("a synthetic dump always serializes")
    }

    fn dumps(logs: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(logs)
            .expect("logs folder exists")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.ends_with(".dmp"))
            .collect();
        names.sort();
        names
    }

    fn only_record(logs: &Path) -> Value {
        let records = super::super::hook::files(logs);
        assert_eq!(records.len(), 1, "one crash gives exactly one record");
        serde_json::from_slice(&fs::read(&records[0]).expect("record readable")).expect("json")
    }

    #[test]
    fn finish_writes_one_record_and_keeps_the_two_newest_dumps() {
        let root = tempfile::tempdir().expect("temp dir");
        let logs = root.path().join("logs");
        let sink = DumpSink::new(&logs).expect("sink");
        for name in [
            "crash-20200101-000000-11.dmp",
            "crash-20200102-000000-12.dmp",
            "crash-20200103-000000-13.dmp",
            "notes.dmp",
        ] {
            fs::write(logs.join(name), b"old").expect("seed dump");
        }
        let (mut file, path) = sink.create().expect("create");
        file.write_all(&access_violation()).expect("write dump");
        drop(file);

        let written = sink.finish(&path).expect("finish");

        let record = only_record(&logs);
        assert_eq!(super::super::hook::files(&logs), vec![written]);
        let message = record["message"].as_str().expect("message");
        assert!(message.starts_with("native crash: "), "{message}");
        assert_eq!(record["location"], "0x1234");
        let created = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            dumps(&logs),
            vec![
                "crash-20200103-000000-13.dmp".to_owned(),
                created,
                "notes.dmp".to_owned()
            ]
        );
    }

    #[test]
    fn unreadable_dump_still_writes_a_record() {
        let root = tempfile::tempdir().expect("temp dir");
        let logs = root.path().join("logs");
        let sink = DumpSink::new(&logs).expect("sink");
        let (mut file, path) = sink.create().expect("create");
        file.write_all(b"not a minidump").expect("write garbage");
        drop(file);

        sink.finish(&path).expect("finish");

        let record = only_record(&logs);
        assert_eq!(record["message"], "native crash: unreadable minidump");
        assert_eq!(record["backtrace"], "");
        assert_eq!(record["thread"], "native");
    }

    #[test]
    fn create_never_overwrites_an_existing_dump() {
        let root = tempfile::tempdir().expect("temp dir");
        let logs = root.path().join("logs");
        let sink = DumpSink::new(&logs).expect("sink");
        let now = chrono::Utc::now();
        // Seed the names for this second and the next two, so the clock cannot step past them.
        let seeded: Vec<_> = (0..3)
            .map(|ahead| {
                let stamp = (now + chrono::Duration::seconds(ahead)).format("%Y%m%d-%H%M%S");
                let path = logs.join(format!("crash-{stamp}-{}.dmp", std::process::id()));
                fs::write(&path, b"earlier crash").expect("seed dump");
                path
            })
            .collect();

        let error = sink.create().expect_err("an existing name is refused");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        for path in &seeded {
            assert_eq!(fs::read(path).expect("seed readable"), b"earlier crash");
        }
    }
}
