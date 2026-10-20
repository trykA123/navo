use super::claims::Claims;
use super::files::plain_section;
use super::from_map::MapIndex;
use super::{Builder, NameRef, Owner, Owners};
use crate::MapModel;
use crate::space::Kind;

#[must_use]
pub fn owners_of_map(map: &MapModel) -> Owners {
    let index = MapIndex::new(map);
    let mut builder = Builder::default();
    for (at, section) in map.sections.iter().enumerate() {
        let kind = Kind::of_name(plain_section(&section.name));
        if section.size > 0 && kind != Kind::Fill {
            claim_section(&index, &mut builder, map, (at, kind));
        }
    }
    builder.items.sort_by_key(|owner| owner.start);
    Owners {
        items: builder.items,
        provenance: builder.provenance,
        files: builder.files.items,
    }
}

fn claim_section(
    index: &MapIndex<'_>,
    out: &mut Builder,
    map: &MapModel,
    (at, kind): (usize, Kind),
) {
    let section = &map.sections[at];
    let span = section.address..section.address.saturating_add(section.size);
    let mut claims = Claims::new(span.clone());
    let owner = |start, size, name, object| Owner {
        start,
        size,
        name,
        kind,
        section: at as u32,
        object,
        source_file: None,
        estimated: false,
        image: false,
    };
    for (n, part) in index.within(span) {
        let is_fill = Kind::of_name(plain_section(&part.section)) == Kind::Fill;
        if is_fill || plain_section(&part.output_section) != plain_section(&section.name) {
            continue;
        }
        let object = (!part.object_file.is_empty()).then(|| out.files.id(&part.object_file));
        let start = part.address.unwrap_or(0);
        for piece in claims.claim(start..start.saturating_add(part.size)) {
            let size = piece.end - piece.start;
            out.items
                .push(owner(piece.start, size, NameRef::Contribution(n), object));
            out.provenance.from_map += 1;
        }
    }
    for gap in claims.gaps() {
        out.items.push(owner(
            gap.start,
            gap.end - gap.start,
            NameRef::Unattributed,
            None,
        ));
        out.provenance.not_attributed += 1;
    }
}
