use std::collections::HashMap;

use super::{MapModel, OutputSection};

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
    let mut sections: HashMap<&str, Option<&OutputSection>> = HashMap::new();
    for section in &model.sections {
        sections
            .entry(&section.name)
            .and_modify(|previous| {
                if previous.is_some_and(|previous| {
                    previous.address != section.address || previous.size != section.size
                }) {
                    *previous = None;
                }
            })
            .or_insert(Some(section));
    }
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
        }
    }
}
