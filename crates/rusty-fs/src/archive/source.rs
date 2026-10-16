use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

use super::damaged;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stop {
    Budget,
    Cancelled,
}

#[derive(Debug)]
pub(crate) enum Fail {
    Stop(Stop),
    Damaged(String),
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self::Damaged(message)
    }
}

impl Fail {
    pub(crate) fn into_read_error(self) -> String {
        match self {
            Self::Stop(Stop::Budget) => super::OVER_BUDGET.to_owned(),
            Self::Stop(Stop::Cancelled) => super::CANCELLED.to_owned(),
            Self::Damaged(message) => message,
        }
    }
}

pub(crate) struct Meter<'a> {
    left: u64,
    cancel: &'a AtomicBool,
    stopped: Option<Stop>,
}

impl<'a> Meter<'a> {
    pub(crate) fn new(budget: u64, cancel: &'a AtomicBool) -> Self {
        Self {
            left: budget,
            cancel,
            stopped: None,
        }
    }

    pub(crate) fn stopped(&self) -> Option<Stop> {
        self.stopped
    }

    fn halt(&mut self, stop: Stop) -> io::Error {
        self.stopped = Some(stop);
        io::Error::other("stopped")
    }

    pub(crate) fn read_from(&mut self, inner: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(self.halt(Stop::Cancelled));
        }
        if self.left == 0 && !buf.is_empty() {
            return Err(self.halt(Stop::Budget));
        }
        let wanted = usize::try_from(self.left).map_or(buf.len(), |left| left.min(buf.len()));
        let read = inner.read(&mut buf[..wanted])?;
        self.left -= read as u64;
        Ok(read)
    }
}

pub(crate) struct Metered<'m, 'a, R> {
    pub inner: R,
    pub meter: &'m mut Meter<'a>,
}

impl<R: Read> Read for Metered<'_, '_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.meter.read_from(&mut self.inner, buf)
    }
}

pub(crate) trait Source: Read {
    fn skip(&mut self, bytes: u64) -> Result<(), Fail>;
    fn stopped(&self) -> Option<Stop>;

    fn fail(&self, error: impl std::fmt::Display) -> Fail {
        self.stopped()
            .map_or_else(|| Fail::Damaged(damaged(error)), Fail::Stop)
    }
}

pub(crate) struct Seekable<'a, R> {
    inner: R,
    len: u64,
    cancel: &'a AtomicBool,
}

impl<'a, R: Read + Seek> Seekable<'a, R> {
    pub(crate) fn new(mut inner: R, cancel: &'a AtomicBool) -> Result<Self, String> {
        let len = inner.seek(SeekFrom::End(0)).map_err(damaged)?;
        inner.rewind().map_err(damaged)?;
        Ok(Self { inner, len, cancel })
    }
}

impl<R: Read> Read for Seekable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read + Seek> Source for Seekable<'_, R> {
    fn skip(&mut self, bytes: u64) -> Result<(), Fail> {
        if let Some(stop) = self.stopped() {
            return Err(Fail::Stop(stop));
        }
        let at = self.inner.stream_position().map_err(damaged)?;
        let to = at.saturating_add(bytes).min(self.len);
        self.inner.seek(SeekFrom::Start(to)).map_err(damaged)?;
        Ok(())
    }

    fn stopped(&self) -> Option<Stop> {
        self.cancel
            .load(Ordering::Relaxed)
            .then_some(Stop::Cancelled)
    }
}

pub(crate) struct Stream<'a, R> {
    inner: R,
    meter: Meter<'a>,
}

impl<'a, R: Read> Stream<'a, R> {
    pub(crate) fn new(inner: R, meter: Meter<'a>) -> Self {
        Self { inner, meter }
    }
}

impl<R: Read> Read for Stream<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.meter.read_from(&mut self.inner, buf)
    }
}

impl<R: Read> Source for Stream<'_, R> {
    fn skip(&mut self, bytes: u64) -> Result<(), Fail> {
        let copied = io::copy(&mut self.by_ref().take(bytes), &mut io::sink());
        copied.map(drop).map_err(|error| self.fail(error))
    }

    fn stopped(&self) -> Option<Stop> {
        self.meter.stopped
    }
}
