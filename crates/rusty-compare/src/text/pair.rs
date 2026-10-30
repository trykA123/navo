use std::collections::HashMap;

use super::side::Line;

/// S2 kept the 60 % pairing threshold; the 50-subhunk sample still needs hand-checking.
pub const PAIR_THRESHOLD: f64 = 0.60;
/// D20: longer subhunks pair in order instead of by distance.
pub const PAIR_MAX_LINES: usize = 200;
/// D7: lines over this many characters are whole-line changes.
pub const PAIR_MAX_LINE_CHARS: usize = 4_096;
/// Stop one distance computation before it can dominate a frame.
pub const PAIR_WORK_LIMIT: usize = 1_000_000;

#[derive(Debug, Clone, Copy)]
pub struct Subhunk<'a> {
    pub left: &'a [Line],
    pub right: &'a [Line],
}

/// A selected pair, as indices into the subhunk's left and right lines.
pub type Pair = (usize, usize);
pub type Pairs = Vec<Pair>;

#[must_use]
pub fn pair_lines(subhunk: Subhunk<'_>) -> Pairs {
    if subhunk.left.len() > PAIR_MAX_LINES || subhunk.right.len() > PAIR_MAX_LINES {
        return in_order(subhunk.left.len(), subhunk.right.len());
    }
    let left: Vec<PreparedLine> = subhunk.left.iter().map(PreparedLine::new).collect();
    let right: Vec<PreparedLine> = subhunk.right.iter().map(PreparedLine::new).collect();
    let mut candidates = Vec::new();
    for (left_index, left_line) in left.iter().enumerate() {
        for (right_index, right_line) in right.iter().enumerate() {
            let Some(distance) = left_line.distance(right_line) else {
                continue;
            };
            let longer = left_line.len.max(right_line.len);
            let limit = (longer as f64 * PAIR_THRESHOLD).floor() as usize;
            if distance <= limit {
                candidates.push((distance, left_index, right_index));
            }
        }
    }
    candidates.sort_unstable();
    greedy(candidates, left.len(), right.len())
}

fn in_order(left: usize, right: usize) -> Pairs {
    (0..left.min(right)).map(|index| (index, index)).collect()
}

fn greedy(candidates: Vec<(usize, usize, usize)>, left: usize, right: usize) -> Pairs {
    let mut used_left = vec![false; left];
    let mut used_right = vec![false; right];
    let mut pairs = Vec::new();
    for (_, left_index, right_index) in candidates {
        if used_left[left_index] || used_right[right_index] {
            continue;
        }
        let ordered = pairs.iter().all(|&(selected_left, selected_right)| {
            (left_index < selected_left) == (right_index < selected_right)
        });
        if !ordered {
            continue;
        }
        used_left[left_index] = true;
        used_right[right_index] = true;
        pairs.push((left_index, right_index));
    }
    pairs.sort_unstable();
    pairs
}

struct LargeMask {
    blocks: usize,
    map: HashMap<char, Vec<u64>>,
    zero: Vec<u64>,
}

struct PreparedLine {
    len: usize,
    chars: Vec<char>,
    masks: Masks,
}

enum Masks {
    AsciiSmall { len: usize, masks: Box<[u64; 128]> },
    Small { len: usize, map: HashMap<char, u64> },
    Large { len: usize, masks: LargeMask },
    TooLong,
}

impl PreparedLine {
    fn new(line: &Line) -> Self {
        let len = line.text.chars().count();
        if len > PAIR_MAX_LINE_CHARS {
            return Self {
                len,
                chars: Vec::new(),
                masks: Masks::TooLong,
            };
        }
        let chars: Vec<char> = line.text.chars().collect();
        let masks = build_masks(&chars);
        Self { len, chars, masks }
    }

    fn distance(&self, other: &Self) -> Option<usize> {
        if matches!(self.masks, Masks::TooLong) || matches!(other.masks, Masks::TooLong) {
            return None;
        }
        let (pattern, text) = if self.len <= other.len {
            (self, other)
        } else {
            (other, self)
        };
        let blocks = pattern.masks.blocks();
        if text.len.saturating_mul(blocks) > PAIR_WORK_LIMIT {
            return None;
        }
        Some(pattern.masks.distance(&text.chars))
    }
}

fn build_masks(chars: &[char]) -> Masks {
    if chars.len() <= 64 {
        if chars.iter().all(|character| (*character as u32) < 128) {
            let mut masks = Box::new([0u64; 128]);
            for (index, character) in chars.iter().enumerate() {
                masks[*character as usize] |= 1u64 << index;
            }
            return Masks::AsciiSmall {
                len: chars.len(),
                masks,
            };
        }
        let mut map = HashMap::new();
        for (index, character) in chars.iter().enumerate() {
            *map.entry(*character).or_insert(0) |= 1u64 << index;
        }
        Masks::Small {
            len: chars.len(),
            map,
        }
    } else {
        let blocks = chars.len().div_ceil(64);
        let mut map: HashMap<char, Vec<u64>> = HashMap::new();
        for (index, character) in chars.iter().enumerate() {
            let mask = map.entry(*character).or_insert_with(|| vec![0; blocks]);
            mask[index / 64] |= 1u64 << (index % 64);
        }
        Masks::Large {
            len: chars.len(),
            masks: LargeMask {
                blocks,
                map,
                zero: vec![0; blocks],
            },
        }
    }
}

impl Masks {
    fn blocks(&self) -> usize {
        match self {
            Self::AsciiSmall { .. } | Self::Small { .. } => 1,
            Self::Large { masks, .. } => masks.blocks,
            Self::TooLong => 0,
        }
    }

    fn distance(&self, text: &[char]) -> usize {
        match self {
            Self::AsciiSmall { len, masks } => distance_ascii(*len, masks, text),
            Self::Small { len, map } => distance_small(*len, map, text),
            Self::Large { len, masks } => distance_large(*len, masks, text),
            Self::TooLong => usize::MAX,
        }
    }
}

fn distance_ascii(pattern_len: usize, masks: &[u64; 128], text: &[char]) -> usize {
    if pattern_len == 0 {
        return text.len();
    }
    let high = 1u64 << (pattern_len - 1);
    let mut pv = u64::MAX;
    let mut mv = 0u64;
    let mut score = pattern_len;
    for character in text {
        let index = *character as usize;
        let eq = if index < 128 { masks[index] } else { 0 };
        let xv = eq | mv;
        let xh = (((eq & pv).wrapping_add(pv)) ^ pv) | eq;
        let mut ph = mv | !(xh | pv);
        let mut mh = pv & xh;
        score += usize::from(ph & high != 0);
        score -= usize::from(mh & high != 0);
        ph = (ph << 1) | 1;
        mh <<= 1;
        pv = mh | !(xv | ph);
        mv = ph & xv;
    }
    score
}

fn distance_small(pattern_len: usize, masks: &HashMap<char, u64>, text: &[char]) -> usize {
    if pattern_len == 0 {
        return text.len();
    }
    let high = 1u64 << (pattern_len - 1);
    let mut pv = u64::MAX;
    let mut mv = 0u64;
    let mut score = pattern_len;
    for character in text {
        let eq = masks.get(character).copied().unwrap_or(0);
        let xv = eq | mv;
        let xh = (((eq & pv).wrapping_add(pv)) ^ pv) | eq;
        let mut ph = mv | !(xh | pv);
        let mut mh = pv & xh;
        score += usize::from(ph & high != 0);
        score -= usize::from(mh & high != 0);
        ph = (ph << 1) | 1;
        mh <<= 1;
        pv = mh | !(xv | ph);
        mv = ph & xv;
    }
    score
}

fn distance_large(pattern_len: usize, masks: &LargeMask, text: &[char]) -> usize {
    let blocks = masks.blocks;
    let high = 1u64 << ((pattern_len - 1) % 64);
    let mut pv = vec![u64::MAX; blocks];
    let mut mv = vec![0u64; blocks];
    let mut xv = vec![0u64; blocks];
    let mut xh = vec![0u64; blocks];
    let mut ph = vec![0u64; blocks];
    let mut mh = vec![0u64; blocks];
    let mut score = pattern_len;
    for character in text {
        let eq = masks
            .map
            .get(character)
            .map(Vec::as_slice)
            .unwrap_or(&masks.zero);
        for index in 0..blocks {
            xv[index] = eq[index] | mv[index];
        }
        let mut carry = 0u64;
        for index in 0..blocks {
            let (sum, first) = (eq[index] & pv[index]).overflowing_add(pv[index]);
            let (sum, second) = sum.overflowing_add(carry);
            carry = u64::from(first || second);
            xh[index] = (sum ^ pv[index]) | eq[index];
        }
        for index in 0..blocks {
            ph[index] = mv[index] | !(xh[index] | pv[index]);
            mh[index] = pv[index] & xh[index];
        }
        score += usize::from(ph[blocks - 1] & high != 0);
        score -= usize::from(mh[blocks - 1] & high != 0);
        let mut ph_carry = 1u64;
        let mut mh_carry = 0u64;
        for index in 0..blocks {
            let ph_next = ph[index] >> 63;
            let mh_next = mh[index] >> 63;
            ph[index] = (ph[index] << 1) | ph_carry;
            mh[index] = (mh[index] << 1) | mh_carry;
            ph_carry = ph_next;
            mh_carry = mh_next;
        }
        for index in 0..blocks {
            pv[index] = mh[index] | !(xv[index] | ph[index]);
            mv[index] = ph[index] & xv[index];
        }
    }
    score
}

#[cfg(test)]
mod tests;
