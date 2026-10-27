use std::time::Instant;

use super::{
    Cancellation, Client, MemoryPart, MemoryReport, PIPE_NAME, PipeError, Query, QueryTerm,
    Request, Response, platform, session,
};
use crate::ntfs::NtfsIndex;
use crate::table::NameTable;
use crate::usn::FrnMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Names,
    FileIds,
    Tree,
    Totals,
    Journal,
    Sessions,
    Process,
}

const PARTS: [(Part, &str); 7] = [
    (Part::Names, "index service names"),
    (Part::FileIds, "index service file ids"),
    (Part::Tree, "index service tree"),
    (Part::Totals, "index service totals"),
    (Part::Journal, "index service journal"),
    (Part::Sessions, "index service pipe sessions"),
    (Part::Process, "index service private bytes"),
];

impl Part {
    pub fn code(self) -> u16 {
        self as u16
    }

    pub fn from_code(code: u16) -> Option<Self> {
        PARTS.get(usize::from(code)).map(|(part, _)| *part)
    }

    pub fn label(self) -> &'static str {
        PARTS[usize::from(self.code())].1
    }
}

impl MemoryReport {
    pub fn add(&mut self, part: Part, bytes: usize, items: usize) {
        let code = part.code();
        let index = self
            .parts
            .iter()
            .position(|found| found.part == code)
            .unwrap_or_else(|| {
                self.parts.push(MemoryPart {
                    part: code,
                    ..MemoryPart::default()
                });
                self.parts.len() - 1
            });
        let found = &mut self.parts[index];
        found.bytes = found.bytes.saturating_add(bytes as u64);
        found.items = found.items.saturating_add(items as u64);
    }

    pub fn known(&self) -> impl Iterator<Item = (Part, MemoryPart)> + '_ {
        self.parts
            .iter()
            .filter_map(|found| Some((Part::from_code(found.part)?, *found)))
    }

    pub fn add_table(&mut self, table: &NameTable) {
        let parts = table.memory_parts();
        let slots = table.slots() as usize;
        self.add(Part::Names, parts.names, table.len());
        self.add(Part::Tree, parts.tree, slots);
        self.add(Part::Totals, parts.totals, slots);
    }

    pub fn add_index(&mut self, index: &NtfsIndex) {
        self.add_table(&index.table);
        self.add(Part::FileIds, file_ids_bytes(&index.ids), index.ids.len());
        let renames = index.pending_renames();
        self.add(Part::Journal, renames.memory_bytes(), renames.len());
    }

    pub fn add_sessions(&mut self) {
        let (bytes, open) = session::memory();
        self.add(Part::Sessions, bytes, open);
    }
}

fn file_ids_bytes(ids: &FrnMap) -> usize {
    ids.capacity() * (size_of::<(u128, u32)>() + 1)
}

impl Request {
    pub fn wants_memory(&self) -> bool {
        matches!(self.terms.as_slice(), [QueryTerm::MemoryV1])
    }
}

impl Client {
    pub fn memory(deadline: Instant) -> Result<MemoryReport, PipeError> {
        let pipe = platform::Pipe::connect_before(PIPE_NAME, deadline)?;
        pipe.write(
            &Query {
                request: Request {
                    terms: vec![QueryTerm::MemoryV1],
                },
                generation: 1,
            },
            Some(deadline),
        )?;
        decode_memory(pipe.read(Some(deadline), &Cancellation::Poll(&|| false))?)
    }
}

fn decode_memory(response: Response) -> Result<MemoryReport, PipeError> {
    match response {
        Response::MemoryV1 {
            generation: 1,
            report,
        } => Ok(report),
        _ => Err(PipeError::InvalidResponse),
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
