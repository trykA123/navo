use std::cmp::Reverse;
use std::collections::HashMap;

use object::elf::{STT_FILE, STT_SECTION};

use super::files::classify;
use super::from_map::MapIndex;
use crate::elf::{Boundaries, estimate};
use crate::space::images::images;
use crate::space::{Kind, OwnerRef, regions_for_elf};
use crate::{Bind, Elf, Lines, MapModel, Place, Symbol, SymbolKind};

pub(super) const NONE: u32 = u32::MAX;

pub(super) struct Context<'a> {
    pub(super) elf: &'a Elf,
    pub(super) lines: Option<&'a Lines>,
    pub(super) map: Option<MapIndex<'a>>,
    pub(super) stripped: bool,
    pub(super) sized: Vec<Vec<u32>>,
    pub(super) estimated: Vec<Vec<(u32, u64)>>,
    pub(super) marks: Vec<(usize, &'a str)>,
    pub(super) file_of: Vec<u32>,
    pub(super) section_file: Vec<u32>,
    crowded: Vec<bool>,
    images: Vec<Option<(u64, u64)>>,
}

fn real_file(index: usize, name: &str) -> u32 {
    if classify(name).is_some() {
        index as u32
    } else {
        NONE
    }
}

fn is_owning(symbol: &Symbol) -> bool {
    !matches!(
        symbol.kind,
        SymbolKind::Other(STT_FILE | STT_SECTION) | SymbolKind::Tls
    )
}

impl<'a> Context<'a> {
    pub(super) fn new(elf: &'a Elf, lines: Option<&'a Lines>, map: Option<&'a MapModel>) -> Self {
        let count = elf.sections.len();
        let mut context = Self {
            elf,
            lines,
            map: map.map(MapIndex::new),
            stripped: true,
            sized: vec![Vec::new(); count],
            estimated: vec![Vec::new(); count],
            marks: elf.file_marks().collect(),
            file_of: vec![NONE; elf.symbols.len()],
            section_file: vec![NONE; count],
            crowded: crowded(elf),
            images: image_table(elf),
        };
        context.scan();
        context.sort();
        context
    }

    pub(super) fn is_per_function(&self, at: usize) -> bool {
        self.crowded[at] && self.sized[at].len() + self.estimated[at].len() <= 1
    }

    fn scan(&mut self) {
        let boundaries = Boundaries::new(self.elf);
        for (at, file) in self.current_files().into_iter().enumerate() {
            self.register(at, file, &boundaries);
        }
    }

    fn current_files(&self) -> Vec<u32> {
        let mut marks = self.marks.iter().enumerate().peekable();
        let mut current = NONE;
        (0..self.elf.symbols.len())
            .map(|at| {
                while let Some((index, (_, name))) = marks.next_if(|(_, mark)| mark.0 <= at) {
                    current = real_file(index, name);
                }
                current
            })
            .collect()
    }

    fn register(&mut self, at: usize, file: u32, boundaries: &Boundaries) {
        let elf = self.elf;
        let symbol = &elf.symbols[at];
        let Place::Section(index) = symbol.place else {
            return;
        };
        let index = index as usize;
        if index >= self.sized.len() || !is_owning(symbol) {
            return;
        }
        self.stripped = false;
        if symbol.bind == Bind::Local {
            self.file_of[at] = file;
            if self.section_file[index] == NONE {
                self.section_file[index] = file;
            }
        }
        if symbol.size > 0 {
            self.sized[index].push(at as u32);
        } else if let Some(size) = estimate(elf, symbol, boundaries.after(symbol)) {
            self.estimated[index].push((at as u32, size));
        }
    }

    fn sort(&mut self) {
        let symbols = &self.elf.symbols;
        for bucket in &mut self.sized {
            bucket.sort_by_key(|at| {
                let symbol = &symbols[*at as usize];
                (
                    symbol.value,
                    symbol.bind == Bind::Local,
                    Reverse(symbol.size),
                    *at,
                )
            });
        }
        for bucket in &mut self.estimated {
            bucket.sort_by_key(|(at, _)| (symbols[*at as usize].value, *at));
        }
    }

    pub(super) fn load_image(&self, at: usize) -> Option<(u64, u64)> {
        self.images[at]
    }
}

fn image_table(elf: &Elf) -> Vec<Option<(u64, u64)>> {
    let mut table = vec![None; elf.sections.len()];
    for image in images(elf) {
        table[image.section] = Some((image.start, image.size));
    }
    table
}

fn crowded(elf: &Elf) -> Vec<bool> {
    let regions = regions_for_elf(elf, None);
    let mut counts: HashMap<(usize, Kind), u32> = HashMap::new();
    let mut keys = vec![None; elf.sections.len()];
    for (at, region) in regions.items.iter().enumerate() {
        for block in &region.blocks {
            if let OwnerRef::ElfSection(section) = block.owner {
                *counts.entry((at, block.kind)).or_default() += 1;
                keys[section] = Some((at, block.kind));
            }
        }
    }
    keys.iter()
        .map(|key| key.is_some_and(|key| counts[&key] > 1))
        .collect()
}
