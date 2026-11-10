//! Natural string order for file names: runs of ASCII digits compare by value.
//! Used by the listing sort in `sort.rs`; pure, no allocation, no integer parsing.

use std::cmp::Ordering;
use std::ops::Range;

/// Compares two (already lowercased) names so that `file2` sorts before `file10`.
/// Digit runs compare by value at any length; other bytes compare as plain text.
/// Only when everything else is equal, fewer leading zeros sort first.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let Some(first) = a.iter().zip(b).position(|(x, y)| x != y) else {
        return a.len().cmp(&b.len());
    };
    let in_digits = a[first].is_ascii_digit() || b[first].is_ascii_digit();
    if !in_digits {
        return a[first].cmp(&b[first]);
    }
    // The shared prefix is identical, so restarting at the digit run's start is safe.
    let start = a[..first]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |at| at + 1);
    natural_from(&a[start..], &b[start..])
}

/// Full run-by-run comparison; the slow path of `natural_cmp`.
fn natural_from(a: &[u8], b: &[u8]) -> Ordering {
    let (mut i, mut j) = (0, 0);
    let mut zeros = Ordering::Equal;
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (run_a, run_b) = (digit_run(&a[i..]), digit_run(&b[j..]));
            let (value_a, value_b) = (strip_zeros(run_a), strip_zeros(run_b));
            let by_value = value_a
                .len()
                .cmp(&value_b.len())
                .then_with(|| value_a.cmp(value_b));
            if by_value != Ordering::Equal {
                return by_value;
            }
            if zeros == Ordering::Equal {
                zeros = (run_a.len() - value_a.len()).cmp(&(run_b.len() - value_b.len()));
            }
            i += run_a.len();
            j += run_b.len();
        } else {
            let by_byte = a[i].cmp(&b[j]);
            if by_byte != Ordering::Equal {
                return by_byte;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j)).then(zeros)
}

/// Returns the leading run of ASCII digits of `bytes`.
fn digit_run(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(bytes.len());
    &bytes[..end]
}

/// Drops leading `0` bytes; an all-zero run becomes empty and equals any other zero run.
fn strip_zeros(run: &[u8]) -> &[u8] {
    let zeros = run.iter().take_while(|byte| **byte == b'0').count();
    &run[zeros..]
}

/// Where one name's key sits in a `NaturalArena`, plus its leading-zero counts per digit run
/// (trailing zero counts left out; empty for most names).
pub struct NaturalRef {
    range: Range<usize>,
    zeros: Box<[u32]>,
}

/// One shared buffer of keys, so a sort of N names makes no per-name allocation for keys.
/// A key is the name with each digit run as a length byte then its significant digits, so
/// plain byte order is the natural order.
#[derive(Default)]
pub struct NaturalArena {
    bytes: Vec<u8>,
}

impl NaturalArena {
    /// Appends the key of a (lowercased) name and returns its handle. Linear in the length.
    pub fn add(&mut self, name: &str) -> NaturalRef {
        let bytes = name.as_bytes();
        let start = self.bytes.len();
        let mut zeros: Vec<u32> = Vec::new();
        let (mut at, mut run_index) = (0, 0_usize);
        while at < bytes.len() {
            if !bytes[at].is_ascii_digit() {
                self.bytes.push(bytes[at]);
                at += 1;
                continue;
            }
            let run = digit_run(&bytes[at..]);
            let value = strip_zeros(run);
            // The first byte stays in '0'..='9' so a run orders against text like a digit does.
            if value.len() < 9 {
                self.bytes.push(b'0' + value.len() as u8);
            } else {
                self.bytes.push(b'9');
                let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
                self.bytes.extend_from_slice(&length.to_be_bytes());
            }
            self.bytes.extend_from_slice(value);
            let leading = u32::try_from(run.len() - value.len()).unwrap_or(u32::MAX);
            if leading > 0 {
                zeros.resize(run_index, 0);
                zeros.push(leading);
            }
            run_index += 1;
            at += run.len();
        }
        NaturalRef {
            range: start..self.bytes.len(),
            zeros: zeros.into_boxed_slice(),
        }
    }

    /// Compares two keys of this arena in natural order.
    pub fn compare(&self, a: &NaturalRef, b: &NaturalRef) -> Ordering {
        self.bytes[a.range.clone()]
            .cmp(&self.bytes[b.range.clone()])
            .then_with(|| a.zeros.cmp(&b.zeros))
    }
}

#[cfg(test)]
mod tests;
