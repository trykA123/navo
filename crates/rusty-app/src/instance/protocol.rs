use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{Error, Reply, Request};

pub(super) const TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const POLL: Duration = Duration::from_millis(5);
const MAX_LINE: usize = 1024 * 1024;

pub(super) fn pause(deadline: Instant, stop: &AtomicBool) -> Result<(), Error> {
    check(deadline, stop)?;
    std::thread::park_timeout(POLL.min(deadline.saturating_duration_since(Instant::now())));
    check(deadline, stop)
}

fn check(deadline: Instant, stop: &AtomicBool) -> Result<(), Error> {
    if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err(Error::Timeout);
    }
    Ok(())
}

pub(super) fn read_line(
    stream: &mut impl Read,
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<String, Error> {
    let mut line = Vec::new();
    let mut buffer = [0_u8; 512];
    loop {
        check(deadline, stop)?;
        match stream.read(&mut buffer) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
            Ok(count) => {
                line.extend_from_slice(&buffer[..count]);
                if line.len() > MAX_LINE {
                    return Err(Error::InvalidRequest);
                }
                if line.contains(&b'\n') {
                    return String::from_utf8(line).map_err(|_| Error::InvalidRequest);
                }
            }
            Err(error) if retry(&error) => pause(deadline, stop)?,
            Err(error) => return Err(error.into()),
        }
    }
}

pub(super) fn write_line(
    stream: &mut impl Write,
    mut line: &[u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<(), Error> {
    while !line.is_empty() {
        check(deadline, stop)?;
        match stream.write(line) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
            Ok(count) => line = &line[count..],
            Err(error) if retry(&error) => pause(deadline, stop)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn exchange(
    stream: &mut (impl Read + Write),
    request: &Request,
    deadline: Instant,
) -> Result<Reply, Error> {
    let stop = AtomicBool::new(false);
    write_line(stream, request.to_line()?.as_bytes(), deadline, &stop)?;
    match read_line(stream, deadline, &stop)?.as_str() {
        "ok\n" => Ok(Reply::Ok),
        "error\n" => Ok(Reply::Error),
        _ => Err(Error::InvalidRequest),
    }
}

pub(super) fn serve(
    stream: &mut (impl Read + Write),
    handler: &impl Fn(Request) -> Reply,
    stop: &AtomicBool,
) {
    let deadline = Instant::now() + TIMEOUT;
    let reply = read_line(stream, deadline, stop)
        .and_then(|line| Request::parse(&line))
        .map_or(Reply::Error, handler);
    let line: &[u8] = match reply {
        Reply::Ok => b"ok\n",
        Reply::Error => b"error\n",
    };
    if let Err(error) = write_line(stream, line, deadline, stop) {
        tracing::debug!(%error, "instance reply was not delivered");
    }
}

fn retry(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}
