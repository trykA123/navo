use std::fs::{File, OpenOptions, TryLockError};
use std::io;
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
        .open(runtime.join(format!("ferrite-{hash}.lock")))?;
    let socket = runtime.join(format!("ferrite-{hash}.sock"));
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
        let listener = self.listener.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "instance server already started",
            )
        })?;
        self.server
            .start(move |stop| serve_connections(&listener, &handler, &stop))
    }
}

fn serve_connections(
    listener: &UnixListener,
    handler: &impl Fn(Request) -> Reply,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(error) = stream.set_nonblocking(true) {
                    tracing::warn!(%error, "cannot configure instance connection");
                    continue;
                }
                protocol::serve(&mut stream, handler, stop);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(protocol::IDLE_POLL);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => {
                tracing::error!(%error, "instance listener failed");
                break;
            }
        }
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
            .name("ferrite-handoff".into())
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
