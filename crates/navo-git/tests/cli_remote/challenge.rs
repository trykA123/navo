use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const CHALLENGE: &[u8] = b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"navo\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// Answers every request with a 401 Basic challenge and counts them.
pub struct Challenge {
    pub port: u16,
    pub hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Challenge {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, halt) = (Arc::clone(&hits), Arc::clone(&stop));
        let thread = thread::spawn(move || serve(&listener, &seen, &halt));
        Self {
            port,
            hits,
            stop,
            thread: Some(thread),
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/repo.git", self.port)
    }
}

fn serve(listener: &TcpListener, hits: &AtomicUsize, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        let Ok((mut stream, _)) = listener.accept() else {
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        stream.set_nonblocking(false).unwrap_or_default();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap_or_default();
        stream.read(&mut [0; 4096]).unwrap_or_default();
        hits.fetch_add(1, Ordering::AcqRel);
        stream.write_all(CHALLENGE).unwrap_or_default();
    }
}

impl Drop for Challenge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap_or_default();
        }
    }
}
