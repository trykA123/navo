use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::server::Server;
use super::{Acquired, Error, Reply, Request, protocol};

pub(crate) struct Guard {
    server: Server,
    listener: Option<UnixListener>,
    socket: PathBuf,
    _lock: File,
}

pub(crate) struct Client {
    socket: PathBuf,
}

pub(super) fn acquire(hash: &str, runtime: &Path) -> Result<Acquired, Error> {
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(runtime.join(format!("{}-{hash}.lock", rusty_core::brand::SLUG)))?;
    let socket = runtime.join(format!("{}-{hash}.sock", rusty_core::brand::SLUG));
    match lock.try_lock() {
        Ok(()) => (),
        Err(TryLockError::WouldBlock) => return Ok(Acquired::Second(Client { socket })),
        Err(TryLockError::Error(error)) => return Err(error.into()),
    }
    remove_socket(&socket)?;
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok(Acquired::First(Guard {
        server: Server::default(),
        listener: Some(listener),
        socket,
        _lock: lock,
    }))
}

fn remove_socket(socket: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(socket) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(socket),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "instance path is not a socket",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

impl Guard {
    pub(crate) fn serve(
        &mut self,
        handler: impl Fn(Request) -> Reply + Send + 'static,
    ) -> Result<(), Error> {
        let (wake, mut signal) = UnixStream::pair()?;
        let listener = self.listener.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "instance server already started",
            )
        })?;
        self.server.start(
            move |stop| serve_connections(&listener, &wake, &handler, &stop),
            move || match signal.write_all(&[1]) {
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                result => result,
            },
        )
    }
}

fn serve_connections(
    listener: &UnixListener,
    wake: &UnixStream,
    handler: &impl Fn(Request) -> Reply,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        match wait_for_connection(listener, wake) {
            Ok(true) if !stop.load(Ordering::Acquire) => (),
            Ok(_) => break,
            Err(error) => {
                tracing::error!(%error, "instance listener wait failed");
                break;
            }
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(error) = stream.set_nonblocking(true) {
                    tracing::warn!(%error, "cannot configure instance connection");
                    continue;
                }
                protocol::serve(&mut stream, handler, stop);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => {
                tracing::error!(%error, "instance listener failed");
                break;
            }
        }
    }
}

fn wait_for_connection(listener: &UnixListener, wake: &UnixStream) -> io::Result<bool> {
    let mut events = [listener.as_raw_fd(), wake.as_raw_fd()].map(|fd| libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    });
    loop {
        // SAFETY: both descriptors stay open and events contains two writable pollfd entries.
        let ready = unsafe { libc::poll(events.as_mut_ptr(), 2, -1) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if events[1].revents != 0 {
            return Ok(false);
        }
        if events[0].revents & libc::POLLIN != 0 {
            return Ok(true);
        }
        return Err(io::Error::other("instance listener is unavailable"));
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.server.stop();
        if let Err(error) = remove_socket(&self.socket) {
            tracing::warn!(%error, "cannot remove instance socket");
        }
    }
}

impl Client {
    pub(crate) fn send(&self, request: &Request, timeout: Duration) -> Result<Reply, Error> {
        let deadline = Instant::now() + timeout;
        let socket = self.socket.clone();
        let request = request.clone();
        let (sender, replies) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("navo-handoff".into())
            .spawn(move || {
                let _ = sender.send(send_to(&socket, &request, deadline));
            })?;
        match replies.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(reply) => reply,
            Err(_) => Err(Error::Timeout),
        }
    }
}

fn send_to(socket: &Path, request: &Request, deadline: Instant) -> Result<Reply, Error> {
    let mut stream = loop {
        match UnixStream::connect(socket) {
            Ok(stream) => break stream,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::Interrupted
                ) =>
            {
                protocol::wait_for_connection(deadline)?;
            }
            Err(error) => return Err(error.into()),
        }
    };
    stream.set_nonblocking(true).map_err(|_| Error::Timeout)?;
    protocol::exchange(&mut stream, request, deadline)
}
