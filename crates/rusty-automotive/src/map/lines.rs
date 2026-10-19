use std::io::{self, BufRead};

pub(super) enum Step {
    Line { bytes: u64, too_long: bool },
    Skip(u64),
    End,
}

pub(super) struct Lines<R> {
    source: R,
    too_long: bool,
    taken: u64,
}

impl<R: BufRead> Lines<R> {
    pub(super) fn new(source: R) -> Self {
        Self {
            source,
            too_long: false,
            taken: 0,
        }
    }

    fn end_line(&mut self, line: &mut Vec<u8>) -> Step {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.len() > super::MAX_MAP_LINE {
            self.too_long = true;
            line.clear();
        }
        Step::Line {
            bytes: std::mem::take(&mut self.taken),
            too_long: std::mem::take(&mut self.too_long),
        }
    }

    fn end_input(&mut self, line: &mut Vec<u8>) -> Step {
        if self.taken > 0 || self.too_long {
            self.end_line(line)
        } else {
            Step::End
        }
    }

    pub(super) fn next(&mut self, line: &mut Vec<u8>) -> io::Result<Step> {
        loop {
            let chunk = self.source.fill_buf()?;
            let chunk = &chunk[..chunk.len().min(64 * 1024)];
            if chunk.is_empty() {
                return Ok(self.end_input(line));
            }
            let end = chunk.iter().position(|byte| *byte == b'\n');
            let body = &chunk[..end.unwrap_or(chunk.len())];
            if line.len() + body.len() > super::MAX_MAP_LINE + 1 {
                self.too_long = true;
                line.clear();
            }
            if !self.too_long {
                line.extend_from_slice(body);
            }
            let take = end.map_or(chunk.len(), |at| at + 1);
            self.source.consume(take);
            self.taken += take as u64;
            if end.is_some() {
                return Ok(self.end_line(line));
            }
            if self.too_long && self.taken >= 64 * 1024 {
                return Ok(Step::Skip(std::mem::take(&mut self.taken)));
            }
        }
    }
}
