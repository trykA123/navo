use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

use ttf_parser::cmap::Subtable4;

use super::sfnt::{u16_at, u32_at, usize_at};

pub(super) const GRID_CHARS: usize = 256;
const FIRST_CHAR: u32 = 0x21;
const LAST_BMP: u32 = 0xFFFF;
const MAX_GROUPS: u32 = 65_536;
const MAX_STEPS: u64 = 1 << 22;
const CANCEL_EVERY: u64 = 4_096;
const HEADER: usize = 4;
const RECORD: usize = 8;
const GROUPS_AT: usize = 16;
const GROUP: usize = 12;
const FORMATS: [u16; 3] = [4, 12, 13];

struct Walk<'a> {
    chars: BTreeSet<char>,
    steps: u64,
    cancel: &'a AtomicBool,
}

impl Walk<'_> {
    fn step(&mut self) -> bool {
        self.steps += 1;
        let cancelled =
            self.steps.is_multiple_of(CANCEL_EVERY) && self.cancel.load(Ordering::Relaxed);
        self.steps <= MAX_STEPS && !cancelled && self.chars.len() < GRID_CHARS
    }

    fn offer(&mut self, code: u32, glyph: u32) {
        let shown = char::from_u32(code).filter(|ch| !ch.is_control() && !ch.is_whitespace());
        if let Some(ch) = shown.filter(|_| glyph != 0) {
            self.chars.insert(ch);
        }
    }

    fn format_4(&mut self, subtable: &[u8]) {
        let Some(table) = Subtable4::parse(subtable) else {
            return;
        };
        for code in FIRST_CHAR..=LAST_BMP {
            if !self.step() {
                return;
            }
            let glyph = table
                .glyph_index(code)
                .map_or(0, |glyph| u32::from(glyph.0));
            self.offer(code, glyph);
        }
    }

    fn group(&mut self, subtable: &[u8], at: usize, is_constant: bool) -> bool {
        let (Some(start), Some(end), Some(first_glyph)) = (
            u32_at(subtable, at),
            u32_at(subtable, at + 4),
            u32_at(subtable, at + 8),
        ) else {
            return false;
        };
        if is_constant && first_glyph == 0 {
            return true;
        }
        for code in start.max(FIRST_CHAR)..=end.min(u32::from(char::MAX)) {
            if !self.step() {
                return false;
            }
            let glyph = if is_constant {
                Some(first_glyph)
            } else {
                first_glyph.checked_add(code - start)
            };
            match glyph.filter(|glyph| *glyph <= u32::from(u16::MAX)) {
                Some(glyph) => self.offer(code, glyph),
                None => return true,
            }
        }
        true
    }

    fn groups(&mut self, subtable: &[u8], is_constant: bool) {
        let count = u32_at(subtable, 12).unwrap_or(0).min(MAX_GROUPS);
        for index in 0..count as usize {
            let at = GROUPS_AT + index * GROUP;
            if !self.step() || !self.group(subtable, at, is_constant) {
                return;
            }
        }
    }
}

fn is_unicode(platform: u16, encoding: u16) -> bool {
    platform == 0 || (platform == 3 && matches!(encoding, 1 | 10))
}

fn unicode_subtables<'a>(cmap: &'a [u8], walk: &mut Walk<'_>) -> [Option<&'a [u8]>; 3] {
    let mut found = [None; 3];
    let count = u16_at(cmap, 2).unwrap_or(0);
    for index in 0..usize::from(count) {
        let at = HEADER + index * RECORD;
        if !walk.step() {
            break;
        }
        let (Some(platform), Some(encoding), Some(offset)) = (
            u16_at(cmap, at),
            u16_at(cmap, at + 2),
            usize_at(cmap, at + 4),
        ) else {
            break;
        };
        let Some(subtable) = cmap
            .get(offset..)
            .filter(|_| is_unicode(platform, encoding))
        else {
            continue;
        };
        let slot = u16_at(subtable, 0).and_then(|format| FORMATS.iter().position(|f| *f == format));
        if let Some(slot) = slot.filter(|slot| found[*slot].is_none()) {
            found[slot] = Some(subtable);
        }
        if found.iter().all(Option::is_some) {
            break;
        }
    }
    found
}

pub(super) fn grid_chars(cmap: &[u8], cancel: &AtomicBool) -> Vec<char> {
    let mut walk = Walk {
        chars: BTreeSet::new(),
        steps: 0,
        cancel,
    };
    let [format_4, format_12, format_13] = unicode_subtables(cmap, &mut walk);
    walk.steps = 0;
    if let Some(subtable) = format_4 {
        walk.format_4(subtable);
    }
    if let Some(subtable) = format_12 {
        walk.groups(subtable, false);
    }
    if let Some(subtable) = format_13 {
        walk.groups(subtable, true);
    }
    walk.chars.into_iter().take(GRID_CHARS).collect()
}
