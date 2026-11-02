use std::io::{self, BufRead};

pub(crate) const MAX_LINE: usize = 1024;
const SKIP_STEP: u64 = 1 << 20;

pub(crate) enum Step {
    Line { len: u64, is_too_long: bool },
    Skip(u64),
    End,
}

pub(crate) struct Lines<R> {
    source: R,
    after_cr: bool,
    is_too_long: bool,
    taken: u64,
}

pub(crate) fn is_line_end(byte: u8) -> bool {
    byte == b'\n' || byte == b'\r'
}

impl<R: BufRead> Lines<R> {
    pub(crate) fn new(source: R) -> Self {
        Self {
            source,
            after_cr: false,
            is_too_long: false,
            taken: 0,
        }
    }

    fn at_end(&mut self) -> Step {
        if self.taken > 0 {
            self.line_done()
        } else {
            Step::End
        }
    }

    fn line_done(&mut self) -> Step {
        let step = Step::Line {
            len: self.taken,
            is_too_long: self.is_too_long,
        };
        self.taken = 0;
        self.is_too_long = false;
        step
    }

    pub(crate) fn next(&mut self, line: &mut Vec<u8>) -> io::Result<Step> {
        loop {
            let chunk = self.source.fill_buf()?;
            let Some(&first) = chunk.first() else {
                return Ok(self.at_end());
            };
            if std::mem::take(&mut self.after_cr) && first == b'\n' && self.taken == 0 {
                self.source.consume(1);
                return Ok(Step::Skip(1));
            }
            let end = chunk.iter().position(|byte| is_line_end(*byte));
            let body = chunk.get(..end.unwrap_or(chunk.len())).unwrap_or_default();
            if !self.is_too_long && line.len() + body.len() > MAX_LINE {
                self.is_too_long = true;
                line.clear();
            }
            if !self.is_too_long {
                line.extend_from_slice(body);
            }
            let ends_with_cr = end.and_then(|at| chunk.get(at)) == Some(&b'\r');
            let take = end.map_or(chunk.len(), |at| at + 1);
            self.source.consume(take);
            self.taken += take as u64;
            if end.is_some() {
                self.after_cr = ends_with_cr;
                return Ok(self.line_done());
            }
            if self.is_too_long && self.taken >= SKIP_STEP {
                return Ok(Step::Skip(std::mem::take(&mut self.taken)));
            }
        }
    }
}
