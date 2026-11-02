use super::parts::Part;
use super::{MapModel, MapProblem};

pub(super) const CONTRIBUTION_TEXT_BYTES: usize = super::MAX_MAP_TEXT_BYTES / 4;

#[derive(Default, Clone, Copy)]
struct Usage {
    entries: usize,
    text_bytes: usize,
    capped: bool,
}

#[derive(Default)]
pub(super) struct Retention {
    kinds: [Usage; 5],
}

impl Retention {
    pub(super) fn capped(&self, part: Part) -> bool {
        let kind = match part {
            Part::Memory => 0,
            Part::Locate => 1,
            Part::Symbols => 2,
            Part::Link => 3,
            Part::Removed => 4,
            Part::Skip => return false,
        };
        self.kinds[kind].capped
    }

    pub(super) fn store(&mut self, model: &mut MapModel, row: Record) -> bool {
        let (kind, cap, text_share) = match &row {
            Record::Memory(_) => (0, super::MAX_MAP_REGIONS, 1),
            Record::Section(_) => (1, super::MAX_MAP_SECTIONS, 4),
            Record::Symbol(_) => (2, super::MAX_MAP_ENTRIES, 16),
            Record::Contribution(_) => (3, super::MAX_MAP_CONTRIBUTIONS, 8),
            Record::Discarded(_) => (4, super::MAX_MAP_DISCARDED, 3),
        };
        let usage = &mut self.kinds[kind];
        if usage.capped {
            return false;
        }
        let text_bytes = row.text_bytes();
        let limit = if usage.entries >= cap {
            Some("retained entries")
        } else if usage.text_bytes + text_bytes > super::MAX_MAP_TEXT_BYTES / 32 * text_share {
            Some("retained text bytes")
        } else {
            None
        };
        if let Some(limit) = limit {
            usage.capped = true;
            model.note(MapProblem::CapReached {
                line: model.lines,
                limit,
            });
            return false;
        }
        usage.entries += 1;
        usage.text_bytes += text_bytes;
        match row {
            Record::Section(row) => model.sections.push(row),
            Record::Symbol(row) => model.symbols.push(row),
            Record::Memory(row) => model.regions.push(row),
            Record::Contribution(row) => model.contributions.push(row),
            Record::Discarded(row) => model.discarded.push(row),
        }
        true
    }
}

pub(super) enum Record {
    Memory(super::MemoryRegion),
    Section(super::OutputSection),
    Symbol(super::Symbol),
    Contribution(super::Contribution),
    Discarded(super::DiscardedSection),
}

impl Record {
    fn text_bytes(&self) -> usize {
        let optional = |text: &Option<String>| text.as_ref().map_or(0, String::len);
        match self {
            Self::Memory(row) => row.name.len(),
            Self::Section(row) => {
                row.name.len() + optional(&row.region) + optional(&row.address_space)
            }
            Self::Symbol(row) => {
                row.name.len() + optional(&row.section) + optional(&row.address_space)
            }
            Self::Contribution(row) => {
                row.object_file.len() + row.section.len() + row.output_section.len()
            }
            Self::Discarded(row) => {
                row.name.len()
                    + optional(&row.object_file)
                    + optional(&row.library)
                    + optional(&row.symbol)
                    + optional(&row.reason)
            }
        }
    }
}
