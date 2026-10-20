use super::asc::{self, Header, Shape};
use super::{Notation, trc};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Clock {
    pub notation: Notation,
    pub now: u64,
    trc: Option<trc::State>,
}

impl Clock {
    pub(crate) fn parse(&mut self, line: &[u8], data: &mut Vec<u8>) -> (Option<u64>, Shape) {
        let line = line
            .strip_prefix(b"\xEF\xBB\xBF")
            .unwrap_or(line)
            .trim_ascii();
        if self.trc.is_none() && trc::is_header(line) {
            self.trc = Some(trc::State::default());
            self.notation = Notation::default();
        }
        if self.trc.is_none() && line.starts_with(b";") {
            return (None, Shape::Header(Header::Other));
        }
        match &mut self.trc {
            Some(state) => state.parse(line, data),
            None => asc::parse(line, self.notation.base, data),
        }
    }

    pub(crate) fn tick(&mut self, time: Option<u64>) -> Option<u64> {
        let time = time?;
        self.now = if self.notation.relative {
            self.now.saturating_add(time)
        } else {
            time
        };
        Some(self.now)
    }

    pub(crate) fn read(&mut self, header: &Header) {
        if let Header::Notation { base, relative } = header {
            self.notation.base = base.unwrap_or(self.notation.base);
            self.notation.relative = relative.unwrap_or(self.notation.relative);
        }
    }
}
