use std::collections::HashMap;

use super::retention::CONTRIBUTION_TEXT_BYTES;
use super::{MapModel, MapProblem, OutputSection};

pub(super) fn finish(model: &mut MapModel) {
    resolve_contributions(model);
    model.symbols.sort_unstable_by(|a, b| {
        (&a.name, a.address, &a.address_space, &a.section, a.size).cmp(&(
            &b.name,
            b.address,
            &b.address_space,
            &b.section,
            b.size,
        ))
    });
    model.symbols.dedup();
}

fn resolve_contributions(model: &mut MapModel) {
    let sections = section_index(&model.sections);
    let mut text_bytes: usize = model
        .contributions
        .iter()
        .map(|part| part.object_file.len() + part.section.len() + part.output_section.len())
        .sum();
    let mut capped = false;
    for part in &mut model.contributions {
        let Some(Some(section)) = sections.get(part.output_section.as_str()) else {
            continue;
        };
        if part
            .offset
            .checked_add(part.size)
            .is_some_and(|end| end <= section.size)
        {
            part.address = section.address.checked_add(part.offset);
            let extra = section.region.as_ref().map_or(0, String::len);
            if text_bytes + extra <= CONTRIBUTION_TEXT_BYTES {
                text_bytes += extra;
                part.region = section.region.clone();
            } else {
                capped = true;
            }
        }
    }
    if capped {
        model.note(MapProblem::CapReached {
            line: model.lines,
            limit: "resolved region text bytes",
        });
    }
}

fn section_index(sections: &[OutputSection]) -> HashMap<&str, Option<&OutputSection>> {
    let mut index: HashMap<&str, Option<&OutputSection>> = HashMap::new();
    for section in sections {
        index
            .entry(&section.name)
            .and_modify(|previous| {
                if previous.is_some_and(|previous| {
                    previous.address != section.address
                        || previous.size != section.size
                        || previous.region != section.region
                        || previous.address_space != section.address_space
                }) {
                    *previous = None;
                }
            })
            .or_insert(Some(section));
    }
    index
}
