use std::io::BufRead;

use csv_core::{ReadRecordResult, ReaderBuilder};

use super::{Dialect, MAX_COLUMNS};
use crate::source::RangeSource;

const OUTPUT: usize = 64 * 1024;
const ENDS: usize = 256;

pub(super) enum Next {
    Record { start: u64 },
    End,
}

pub(super) struct Row {
    bytes: Vec<u8>,
    ends: Vec<usize>,
    seen: usize,
    pub fields: usize,
    pub budget: usize,
}

impl Row {
    pub(super) fn new(budget: usize) -> Self {
        Self {
            bytes: Vec::new(),
            ends: Vec::new(),
            seen: 0,
            fields: 0,
            budget,
        }
    }

    fn take(&mut self, output: &[u8], ends: &[usize]) {
        for end in ends {
            self.fields += 1;
            if self.ends.len() < MAX_COLUMNS {
                self.ends.push(self.seen + end);
            }
        }
        let room = self.budget.saturating_sub(self.bytes.len());
        self.bytes
            .extend_from_slice(output.get(..output.len().min(room)).unwrap_or_default());
        self.seen += output.len();
    }

    pub(super) fn is_clipped(&self) -> bool {
        self.seen > self.bytes.len()
    }

    pub(super) fn stored(&self) -> usize {
        self.bytes.len()
    }

    pub(super) fn cells(&self) -> Vec<String> {
        let mut start = 0;
        self.ends
            .iter()
            .map(|end| {
                let end = (*end).min(self.bytes.len());
                let cell = self.bytes.get(start.min(end)..end).unwrap_or_default();
                start = end;
                String::from_utf8_lossy(cell).into_owned()
            })
            .collect()
    }
}

pub(super) struct Records<'r, 'f> {
    parser: csv_core::Reader,
    source: RangeSource<'r, 'f>,
    offset: u64,
    output: Vec<u8>,
    ends: [usize; ENDS],
}

impl<'r, 'f> Records<'r, 'f> {
    pub(super) fn new(source: RangeSource<'r, 'f>, start: u64, dialect: Dialect) -> Self {
        Self {
            parser: ReaderBuilder::new().delimiter(dialect.delimiter).build(),
            source,
            offset: start,
            output: vec![0; OUTPUT],
            ends: [0; ENDS],
        }
    }

    pub(super) fn offset(&self) -> u64 {
        self.offset
    }

    pub(super) fn next(&mut self, mut row: Option<&mut Row>) -> Result<Next, String> {
        let start = self.offset;
        loop {
            let input = self.source.fill_buf().map_err(|error| error.to_string())?;
            let (result, read, written, ended) =
                self.parser
                    .read_record(input, &mut self.output, &mut self.ends);
            self.source.consume(read);
            self.offset += read as u64;
            if let Some(row) = row.as_deref_mut() {
                row.take(
                    self.output.get(..written).unwrap_or_default(),
                    self.ends.get(..ended).unwrap_or_default(),
                );
            }
            match result {
                ReadRecordResult::Record => return Ok(Next::Record { start }),
                ReadRecordResult::End => return Ok(Next::End),
                ReadRecordResult::InputEmpty
                | ReadRecordResult::OutputFull
                | ReadRecordResult::OutputEndsFull => {}
            }
        }
    }
}
