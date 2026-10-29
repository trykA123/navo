use std::io::{self, Write};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, PoisonError, TryLockError, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use super::rolling::RollingFile;

pub(super) struct BufferedFile {
    file: Arc<Mutex<RollingFile>>,
    stop: SyncSender<()>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Default)]
pub struct LogFlush(Weak<BufferedFile>);

impl LogFlush {
    pub fn flush(&self) {
        if let Some(writer) = self.0.upgrade() {
            flush(&writer.file);
        }
    }

    pub fn try_flush(&self) {
        let Some(writer) = self.0.upgrade() else {
            return;
        };
        let result = match writer.file.try_lock() {
            Ok(mut file) => file.flush(),
            Err(TryLockError::Poisoned(error)) => error.into_inner().flush(),
            Err(TryLockError::WouldBlock) => return,
        };
        report(result);
    }

    pub fn shutdown(&self) {
        if let Some(writer) = self.0.upgrade() {
            writer.shutdown();
        }
    }
}

impl BufferedFile {
    pub(super) fn new(file: RollingFile) -> io::Result<Arc<Self>> {
        let file = Arc::new(Mutex::new(file));
        let background = Arc::clone(&file);
        let (stop, stopped) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("navo-log-flush".to_owned())
            .spawn(move || {
                while let Err(RecvTimeoutError::Timeout) =
                    stopped.recv_timeout(Duration::from_secs(1))
                {
                    flush(&background);
                }
                flush(&background);
            })?;
        Ok(Arc::new(Self {
            file,
            stop,
            thread: Mutex::new(Some(thread)),
        }))
    }

    pub(super) fn handle(self: &Arc<Self>) -> LogFlush {
        LogFlush(Arc::downgrade(self))
    }

    pub(super) fn write_line(&self, line: &str, is_urgent: bool) {
        let (written, flushed) = {
            let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
            let written = file.write_line(line);
            let flushed = if is_urgent { file.flush() } else { Ok(()) };
            (written, flushed)
        };
        report(written);
        report(flushed);
    }

    fn shutdown(&self) {
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(thread) = thread {
            let _ = self.stop.try_send(());
            if thread.join().is_err() {
                report(Err(io::Error::other("log flush thread panicked")));
            }
        }
        flush(&self.file);
    }
}

impl Drop for BufferedFile {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn flush(file: &Mutex<RollingFile>) {
    let result = file.lock().unwrap_or_else(PoisonError::into_inner).flush();
    report(result);
}

fn report(result: io::Result<()>) {
    if let Err(error) = result {
        let _ = writeln!(io::stderr(), "navo: writing the log failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_flush_returns_while_the_log_mutex_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let file = RollingFile::open(dir.path().to_owned(), "navo".to_owned(), 1024, 2).unwrap();
        let writer = BufferedFile::new(file).unwrap();
        writer.write_line("buffered", false);
        let handle = writer.handle();
        let (locked, acquired) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let holding = writer.clone();
        let holder = std::thread::spawn(move || {
            let _guard = holding.file.lock().unwrap();
            locked.send(()).unwrap();
            released.recv().unwrap();
        });
        acquired.recv().unwrap();
        let (finished, flushed) = mpsc::channel();
        let flushing = handle.clone();
        let thread = std::thread::spawn(move || {
            flushing.try_flush();
            finished.send(()).unwrap();
        });
        let result = flushed.recv_timeout(Duration::from_secs(2));
        release.send(()).unwrap();
        holder.join().unwrap();
        thread.join().unwrap();
        assert!(result.is_ok(), "panic flushing must skip a locked log");
        handle.try_flush();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("navo.log")).unwrap(),
            "buffered\n"
        );
    }
}
