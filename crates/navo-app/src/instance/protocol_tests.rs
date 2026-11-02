use std::io::{self, Read, Write};
use std::time::Instant;

use super::protocol::{TIMEOUT, exchange};
use super::{Error, Request};

struct FailedConnection {
    writing: bool,
    kind: io::ErrorKind,
}

impl Read for FailedConnection {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        if self.kind == io::ErrorKind::UnexpectedEof {
            return Ok(0);
        }
        Err(self.kind.into())
    }
}

impl Write for FailedConnection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.writing {
            return Err(self.kind.into());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn failures_after_connecting_are_reported_as_unanswered_requests() {
    for (writing, kind) in [
        (true, io::ErrorKind::BrokenPipe),
        (false, io::ErrorKind::ConnectionReset),
        (false, io::ErrorKind::UnexpectedEof),
    ] {
        let mut stream = FailedConnection { writing, kind };
        let request = Request {
            experimental_retention: false,
            path: None,
            left: None,
            right: None,
            view: None,
        };
        let result = exchange(&mut stream, &request, Instant::now() + TIMEOUT);
        assert!(
            matches!(result, Err(Error::Timeout)),
            "{kind:?}: {result:?}"
        );
    }
}
