use std::ops::Range;

use navo_core::encoding::{Encoding, encode};
use navo_core::paging::{ByteSource, TextLayout, read_full};

use super::super::side::LineEnding;
use super::ApplyError;
use super::target::{Target, Writer, read_block};

pub(super) struct Copier<'a, 't> {
    target: &'a Target<'t>,
    source: &'a dyn ByteSource,
    eof: bool,
    bom: u64,
}

impl<'a, 't> Copier<'a, 't> {
    pub(super) fn new(target: &'a Target<'t>, source: &'a dyn ByteSource, eof: bool) -> Self {
        let layout = source.layout();
        let bom = if layout.text.bom {
            u64::try_from(layout.bom_len()).unwrap_or(0)
        } else {
            0
        };
        Self {
            target,
            source,
            eof,
            bom,
        }
    }

    pub(super) fn range<'w>(
        &self,
        range: Range<u64>,
        writer: &'w mut Writer<'t>,
        buffer: &mut [u8],
    ) -> Result<(), ApplyError> {
        let start = range.start.max(self.bom);
        if start > range.end {
            if range.end <= self.bom {
                return Ok(());
            }
            return Err(ApplyError::Range);
        }
        if start == range.end {
            return Ok(());
        }
        let layout = self.source.layout();
        let mut sink = LineSink {
            target: self.target,
            encoding: layout.text.encoding,
            same: layout.text.encoding == self.target.plain.encoding,
            eof: self.eof,
            writer,
            decoder: None,
            started: false,
        };
        stream_lines(self.source, start..range.end, buffer, &mut sink)
    }

    pub(super) fn line<'w>(
        &self,
        input: LineInput,
        writer: &'w mut Writer<'t>,
        buffer: &mut [u8],
    ) -> Result<(), ApplyError> {
        let start = input.range.start.max(self.bom);
        if start > input.range.end {
            if input.range.end <= self.bom {
                return Ok(());
            }
            return Err(ApplyError::Range);
        }
        let layout = self.source.layout();
        let (body_end, _) = line_ending(self.source, start..input.range.end, layout)?;
        let mut sink = LineSink {
            target: self.target,
            encoding: layout.text.encoding,
            same: layout.text.encoding == self.target.plain.encoding,
            eof: self.eof,
            writer,
            decoder: None,
            started: false,
        };
        let mut at = start;
        while at < body_end {
            let read = read_block(self.source, at, body_end, buffer)?;
            sink.body(&buffer[..read])?;
            at = at.saturating_add(u64::try_from(read).map_err(|_| ApplyError::Range)?);
        }
        sink.ending(input.is_last)
    }
}

pub(super) struct LineInput {
    pub(super) range: Range<u64>,
    pub(super) is_last: bool,
}

trait Sink {
    fn body(&mut self, bytes: &[u8]) -> Result<(), ApplyError>;
    fn ending(&mut self, is_last: bool) -> Result<(), ApplyError>;
}

struct LineSink<'a, 'w, 't> {
    target: &'a Target<'t>,
    encoding: &'static Encoding,
    same: bool,
    eof: bool,
    writer: &'w mut Writer<'t>,
    decoder: Option<Decoder>,
    started: bool,
}

impl Sink for LineSink<'_, '_, '_> {
    fn body(&mut self, bytes: &[u8]) -> Result<(), ApplyError> {
        if !self.started {
            self.writer.finish_line()?;
            self.started = true;
        }
        if self.same {
            return self.writer.write_all(bytes);
        }
        let encoding = self.encoding;
        let decoder = self.decoder.get_or_insert_with(|| Decoder::new(encoding));
        let text = decoder.feed(bytes, false)?;
        let bytes = encode(&text, &self.target.plain)?;
        self.writer.write_all(&bytes)
    }

    fn ending(&mut self, is_last: bool) -> Result<(), ApplyError> {
        if !self.started {
            self.writer.finish_line()?;
            self.started = true;
        }
        if let Some(mut decoder) = self.decoder.take() {
            let text = decoder.feed(&[], true)?;
            if !text.is_empty() {
                let bytes = encode(&text, &self.target.plain)?;
                self.writer.write_all(&bytes)?;
            }
        }
        if !(is_last && self.eof) {
            self.writer.end_line()?;
        }
        self.started = false;
        Ok(())
    }
}

type Feed = dyn FnMut(&[u8], bool) -> Result<String, ApplyError>;

struct Decoder {
    feed: Box<Feed>,
}

impl Decoder {
    fn new(encoding: &'static Encoding) -> Self {
        let mut decoder = encoding.new_decoder_without_bom_handling();
        let feed = move |bytes: &[u8], last: bool| -> Result<String, ApplyError> {
            let needed = decoder.max_utf8_buffer_length(bytes.len()).unwrap_or(0);
            let mut text = String::with_capacity(needed);
            let (_, read, errors) = decoder.decode_to_string(bytes, &mut text, last);
            if read != bytes.len() {
                return Err(ApplyError::Read("the file ended early".to_owned()));
            }
            if errors {
                return Err(ApplyError::Malformed);
            }
            Ok(text)
        };
        Self {
            feed: Box::new(feed),
        }
    }

    fn feed(&mut self, bytes: &[u8], last: bool) -> Result<String, ApplyError> {
        (self.feed)(bytes, last)
    }
}

fn stream_lines(
    source: &dyn ByteSource,
    range: Range<u64>,
    buffer: &mut [u8],
    sink: &mut impl Sink,
) -> Result<(), ApplyError> {
    let layout = source.layout();
    let unit = layout.unit();
    let mut pending = Vec::with_capacity(unit);
    let mut at = range.start;
    while at < range.end {
        let read = read_block(source, at, range.end, buffer)?;
        let chunk = &buffer[..read];
        let mut start = 0;
        for newline in layout.newlines(chunk) {
            feed_line(sink, &mut pending, &chunk[start..newline], layout)?;
            let line_end = at
                .saturating_add(u64::try_from(newline).map_err(|_| ApplyError::Range)?)
                .saturating_add(u64::try_from(unit).map_err(|_| ApplyError::Range)?);
            sink.ending(line_end == range.end)?;
            start = newline + unit;
        }
        pending.extend_from_slice(&chunk[start..]);
        let more =
            at.saturating_add(u64::try_from(read).map_err(|_| ApplyError::Range)?) < range.end;
        if more && pending.len() > unit {
            let split = pending.len() - unit;
            sink.body(&pending[..split])?;
            pending.drain(..split);
        }
        at = at.saturating_add(u64::try_from(read).map_err(|_| ApplyError::Range)?);
    }
    if !pending.is_empty() {
        sink.body(&pending)?;
        sink.ending(true)?;
    }
    Ok(())
}

fn feed_line(
    sink: &mut impl Sink,
    pending: &mut Vec<u8>,
    segment: &[u8],
    layout: TextLayout,
) -> Result<(), ApplyError> {
    let unit = layout.unit();
    if segment.len() >= unit {
        if !pending.is_empty() {
            sink.body(pending)?;
            pending.clear();
        }
        if layout.is_cr(&segment[segment.len() - unit..]) {
            sink.body(&segment[..segment.len() - unit])?;
        } else {
            sink.body(segment)?;
        }
        return Ok(());
    }
    let mut joined = [0u8; 4];
    let total = pending.len() + segment.len();
    joined[..pending.len()].copy_from_slice(pending);
    joined[pending.len()..total].copy_from_slice(segment);
    let crlf = total >= unit && layout.is_cr(&joined[total - unit..total]);
    let body = if crlf { total - unit } else { total };
    sink.body(&joined[..body])?;
    pending.clear();
    Ok(())
}

fn line_ending(
    source: &dyn ByteSource,
    range: Range<u64>,
    layout: TextLayout,
) -> Result<(u64, LineEnding), ApplyError> {
    let unit = u64::try_from(layout.unit()).map_err(|_| ApplyError::Range)?;
    if range.end < range.start.saturating_add(unit) {
        return Ok((range.end, LineEnding::None));
    }
    let body_end = range.end - unit;
    let mut tail = [0u8; 2];
    read_exact_at(source, body_end, &mut tail[..layout.unit()])?;
    if !layout.is_lf(&tail[..layout.unit()]) {
        return Ok((range.end, LineEnding::None));
    }
    if body_end >= range.start.saturating_add(unit) {
        let mut cr = [0u8; 2];
        read_exact_at(source, body_end - unit, &mut cr[..layout.unit()])?;
        if layout.is_cr(&cr[..layout.unit()]) {
            return Ok((body_end - unit, LineEnding::CrLf));
        }
    }
    Ok((body_end, LineEnding::Lf))
}

fn read_exact_at(source: &dyn ByteSource, at: u64, buf: &mut [u8]) -> Result<(), ApplyError> {
    let read = read_full(source, at, buf).map_err(ApplyError::Read)?;
    if read != buf.len() {
        return Err(ApplyError::Read("the file ended early".to_owned()));
    }
    Ok(())
}
