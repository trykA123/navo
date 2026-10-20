use std::ops::Range;

use super::files::{FileKind, classify, plain_section};
use super::prepare::{Context, NONE};
use super::{Builder, NameRef, Owner};
use crate::space::Kind;
use crate::{Section, Symbol};

#[derive(Clone, Copy)]
struct Piece {
    name: NameRef,
    object: Option<u32>,
    source: Option<u32>,
    estimated: bool,
}

struct Work<'a> {
    ctx: &'a Context<'a>,
    out: &'a mut Builder,
    claims: super::claims::Claims,
    at: usize,
    section: &'a Section,
    kind: Kind,
}

pub(super) fn build(ctx: &Context<'_>, out: &mut Builder, at: usize) {
    let section = &ctx.elf.sections[at];
    let first = out.items.len();
    let mut work = Work {
        ctx,
        out,
        claims: super::claims::Claims::new(section.span()),
        at,
        section,
        kind: Kind::of_section(section),
    };
    work.claim_map();
    if ctx.stripped || ctx.is_per_function(at) {
        work.own_whole();
    } else {
        work.claim_symbols();
        if work.claims.is_empty() {
            work.own_whole();
        }
    }
    work.fill_gaps();
    work.mirror_load_image(first);
}

fn thumb_start(ctx: &Context<'_>, symbol: &Symbol) -> u64 {
    let is_thumb =
        ctx.elf.header.machine == object::elf::EM_ARM && symbol.kind == crate::SymbolKind::Func;
    if is_thumb {
        symbol.value & !1
    } else {
        symbol.value
    }
}

impl Work<'_> {
    fn take(&mut self, want: Range<u64>, piece: Piece) -> bool {
        let pieces = self.claims.claim(want);
        for range in &pieces {
            let source = self.dwarf_source(range.start).or(piece.source);
            self.out.items.push(Owner {
                start: range.start,
                size: range.end - range.start,
                name: piece.name,
                kind: self.kind,
                section: self.at as u32,
                object: piece.object,
                source_file: source,
                estimated: piece.estimated,
                image: false,
            });
        }
        !pieces.is_empty()
    }

    fn dwarf_source(&mut self, address: u64) -> Option<u32> {
        if !self.section.is_executable() {
            return None;
        }
        let (file, _) = self.ctx.lines?.at(address)?;
        Some(self.out.files.id(file))
    }

    fn file_piece(&mut self, file: u32) -> (Option<u32>, Option<u32>) {
        if file == NONE {
            return (None, None);
        }
        let name = self.ctx.marks[file as usize].1;
        match classify(name) {
            Some(FileKind::Object) => (Some(self.out.files.id(name)), None),
            Some(FileKind::Source) => (None, Some(self.out.files.id(name))),
            None => (None, None),
        }
    }

    fn claim_map(&mut self) {
        let Some(index) = &self.ctx.map else {
            return;
        };
        let name = self.section.name.as_str();
        for (at, part) in index.within(self.section.span()) {
            if plain_section(&part.output_section) != plain_section(name) {
                continue;
            }
            let start = part.address.unwrap_or(0);
            let object = Some(self.out.files.id(&part.object_file));
            let piece = Piece {
                name: NameRef::Contribution(at),
                object,
                source: None,
                estimated: false,
            };
            if self.take(start..start.saturating_add(part.size), piece) {
                self.out.provenance.from_map += 1;
            }
        }
    }

    fn own_whole(&mut self) {
        let (object, source) = self.file_piece(self.ctx.section_file[self.at]);
        let piece = Piece {
            name: NameRef::Section(self.at as u32),
            object,
            source,
            estimated: false,
        };
        if self.take(self.section.span(), piece) {
            self.out.provenance.from_sections += 1;
        }
    }

    fn claim_symbols(&mut self) {
        let ctx = self.ctx;
        for &at in &ctx.sized[self.at] {
            let symbol = &ctx.elf.symbols[at as usize];
            let start = thumb_start(ctx, symbol);
            if self.take_symbol(at, start..start.saturating_add(symbol.size), false) {
                self.out.provenance.from_symbols += 1;
            }
        }
        for &(at, size) in &ctx.estimated[self.at] {
            let symbol = &ctx.elf.symbols[at as usize];
            let start = thumb_start(ctx, symbol);
            if self.take_symbol(at, start..symbol.value.saturating_add(size), true) {
                self.out.provenance.estimated += 1;
            }
        }
    }

    fn take_symbol(&mut self, at: u32, want: Range<u64>, estimated: bool) -> bool {
        let (object, source) = self.file_piece(self.ctx.file_of[at as usize]);
        let piece = Piece {
            name: NameRef::Symbol(at),
            object,
            source,
            estimated,
        };
        self.take(want, piece)
    }

    fn fill_gaps(&mut self) {
        for gap in self.claims.gaps() {
            let piece = Piece {
                name: NameRef::Unattributed,
                object: None,
                source: None,
                estimated: false,
            };
            if self.take(gap, piece) {
                self.out.provenance.not_attributed += 1;
            }
        }
    }

    fn mirror_load_image(&mut self, first: usize) {
        if self.kind != Kind::Data {
            return;
        }
        let Some((load, limit)) = self.ctx.load_image(self.at) else {
            return;
        };
        let base = self.section.address;
        let copies: Vec<Owner> = self.out.items[first..]
            .iter()
            .filter(|owner| owner.start - base < limit)
            .map(|owner| Owner {
                start: load + (owner.start - base),
                size: owner.size.min(limit - (owner.start - base)),
                image: true,
                ..*owner
            })
            .collect();
        if !copies.is_empty() {
            self.out.provenance.load_images += 1;
            self.out.items.extend(copies);
        }
    }
}
