use std::ops::Range;

use super::Kind;
use super::kind::takes_no_address_space;
use crate::Elf;

#[derive(Debug, Clone, Copy)]
pub(super) struct Image {
    pub(super) section: usize,
    pub(super) start: u64,
    pub(super) size: u64,
}

impl Image {
    fn span(&self) -> Range<u64> {
        self.start..self.start + self.size
    }
}

pub(super) fn is_placed(elf: &Elf, at: usize) -> bool {
    let section = &elf.sections[at];
    section.is_loaded() && section.size > 0 && !takes_no_address_space(section)
}

pub(super) fn images(elf: &Elf) -> Vec<Image> {
    let mut placed: Vec<Range<u64>> = (0..elf.sections.len())
        .filter(|&at| is_placed(elf, at))
        .map(|at| elf.sections[at].span())
        .collect();
    placed.sort_by_key(|span| span.start);
    let mut reach = 0;
    let reaches: Vec<u64> = placed
        .iter()
        .map(|span| {
            reach = reach.max(span.end);
            reach
        })
        .collect();
    let mut found: Vec<Image> = Vec::new();
    for at in (0..elf.sections.len()).filter(|&at| is_placed(elf, at)) {
        if Kind::of_section(&elf.sections[at]) != Kind::Data {
            continue;
        }
        let Some((start, size)) = load_image(elf, at) else {
            continue;
        };
        let image = Image {
            section: at,
            start,
            size,
        };
        let span = image.span();
        let below = placed.partition_point(|other| other.start < span.end);
        let on_section = below > 0 && reaches[below - 1] > span.start;
        let on_image = found.iter().any(|other| {
            let other = other.span();
            other.start < span.end && span.start < other.end
        });
        if !on_section && !on_image {
            found.push(image);
        }
    }
    found
}

fn load_image(elf: &Elf, at: usize) -> Option<(u64, u64)> {
    let section = &elf.sections[at];
    let segment = elf.load_segments().find(|segment| {
        segment.virtual_address <= section.address
            && section.span().end <= segment.span().end
            && segment.physical_address != segment.virtual_address
    })?;
    let offset = section.address - segment.virtual_address;
    let kept = segment.file_size.saturating_sub(offset).min(section.size);
    let start = segment.physical_address.checked_add(offset)?;
    start.checked_add(kept)?;
    (kept > 0).then_some((start, kept))
}
