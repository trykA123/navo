//! The plain keyword table: words of up to eight ASCII bytes live in a packed open-addressing
//! table, everything else in a hash map. Part of the rule lexer's hot path.

use std::collections::HashMap;

use super::scan::{END, WORD, WORD_BYTES, word_len};
use crate::TokenKind;

/// Multiplier that spreads packed keys over the table (the Fx hash constant).
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
/// Longest word, in bytes, that fits one packed `u64` key.
const SHORT: usize = 8;
/// Longest ASCII word folded on the stack in the long path; longer words allocate.
const FOLD_MAX: usize = 64;

#[derive(Debug, Clone, Copy)]
struct Slot {
    /// The word's bytes, little-endian and zero padded; zero marks an empty slot.
    key: u64,
    kind: TokenKind,
}

/// Plain keywords by exact (or ASCII case-folded) spelling.
#[derive(Debug, Clone)]
pub(super) struct Words {
    case_ignored: bool,
    short: Vec<Slot>,
    shift: u32,
    long: HashMap<String, TokenKind>,
}

/// Marks (bit 7 of each byte) the bytes of `window` that are not ASCII word bytes.
fn non_word_bytes(window: u64) -> u64 {
    const HIGH: u64 = 0x8080_8080_8080_8080;
    const ONES: u64 = 0x0101_0101_0101_0101;
    // Bit 7 of each byte is set when that byte is >= n; no borrow crosses bytes for n < 0x80.
    let at_least = |n: u64| (window | HIGH).wrapping_sub(n * ONES) & HIGH;
    let between = |low: u64, high: u64| at_least(low) & !at_least(high);
    let word =
        between(0x30, 0x3A) | between(0x41, 0x5B) | between(0x61, 0x7B) | between(0x5F, 0x60);
    (!word | window) & HIGH
}

/// Packs up to eight word bytes into a `u64` key, or `None` when the word is longer.
fn pack(word: &[u8]) -> Option<u64> {
    if word.len() > SHORT {
        return None;
    }
    if let Some(head) = word.first_chunk::<SHORT>() {
        return Some(u64::from_le_bytes(*head));
    }
    Some(
        word.iter()
            .rev()
            .fold(0, |key, &byte| key << 8 | u64::from(byte)),
    )
}

/// Adds the ASCII `byte` at word offset `index` to the packed `key`; bytes past the eighth are
/// left out, because longer words are looked up by their text.
pub(super) fn pack_byte(key: u64, byte: u8, index: usize) -> u64 {
    match u32::try_from(index) {
        Ok(index) if index < SHORT as u32 => key | u64::from(byte) << (8 * index),
        _ => key,
    }
}

impl Words {
    /// Builds the table; the first entry wins when two spell the same word.
    pub(super) fn new<'a>(
        case_ignored: bool,
        entries: impl IntoIterator<Item = (&'a String, TokenKind)>,
    ) -> Self {
        let folded: Vec<(String, TokenKind)> = entries
            .into_iter()
            .map(|(word, kind)| {
                let word = if case_ignored {
                    word.to_lowercase()
                } else {
                    word.clone()
                };
                (word, kind)
            })
            .collect();
        let slots = (folded.len() * 2).next_power_of_two().max(2);
        let mut words = Self {
            case_ignored,
            short: vec![
                Slot {
                    key: 0,
                    kind: TokenKind::Keyword
                };
                slots
            ],
            shift: 64 - slots.trailing_zeros(),
            long: HashMap::new(),
        };
        for (word, kind) in folded {
            match Self::short_key(&word) {
                Some(key) => words.insert_short(key, kind),
                None => {
                    words.long.entry(word).or_insert(kind);
                }
            }
        }
        words
    }

    /// The packed key for an ASCII word of at most eight bytes.
    fn short_key(word: &str) -> Option<u64> {
        pack(word.as_bytes()).filter(|_| word.is_ascii())
    }

    fn start(&self, key: u64) -> usize {
        usize::try_from(key.wrapping_mul(SEED) >> self.shift).unwrap_or(0)
    }

    fn insert_short(&mut self, key: u64, kind: TokenKind) {
        let mask = self.short.len() - 1;
        let mut at = self.start(key);
        loop {
            let Some(slot) = self.short.get_mut(at) else {
                return;
            };
            if slot.key == 0 {
                *slot = Slot { key, kind };
                return;
            }
            if slot.key == key {
                return;
            }
            at = (at + 1) & mask;
        }
    }

    fn find_short(&self, key: u64) -> Option<TokenKind> {
        if key == 0 {
            return None;
        }
        let mask = self.short.len() - 1;
        let mut at = self.start(key);
        loop {
            let slot = self.short.get(at)?;
            if slot.key == key {
                return Some(slot.kind);
            }
            if slot.key == 0 {
                return None;
            }
            at = (at + 1) & mask;
        }
    }

    /// Scans the word at the start of `text` and returns its byte length and keyword kind.
    /// Packs the key while counting bytes, so a short ASCII word is read once. Empty words match nothing.
    pub(super) fn find(&self, text: &str) -> (usize, Option<TokenKind>) {
        if let Some(head) = text.as_bytes().first_chunk::<SHORT>() {
            let window = u64::from_le_bytes(*head);
            let stops = non_word_bytes(window);
            // A set bit below bit 64 means the word ends inside the window, on an ASCII byte.
            let end = (stops.trailing_zeros() / 8) as usize;
            if stops != 0 && window.to_le_bytes().get(end).is_some_and(u8::is_ascii) {
                let key = window & !(u64::MAX << (8 * end as u32));
                return (end, self.lookup(key, end, text));
            }
        }
        let mut key = 0_u64;
        for (end, &byte) in text.as_bytes().iter().enumerate() {
            match WORD_BYTES[usize::from(byte)] {
                WORD => key = pack_byte(key, byte, end),
                END => return (end, self.lookup(key, end, text)),
                _ => {
                    let len = word_len(text);
                    return (len, text.get(..len).and_then(|word| self.kind_long(word)));
                }
            }
        }
        (text.len(), self.lookup(key, text.len(), text))
    }

    /// Looks up an ASCII word of `len` bytes whose first eight bytes are packed in `key`.
    fn lookup(&self, mut key: u64, len: usize, text: &str) -> Option<TokenKind> {
        if len == 0 {
            return None;
        }
        if len > SHORT {
            return text.get(..len).and_then(|word| self.kind_long(word));
        }
        if self.case_ignored {
            key = u64::from_le_bytes(key.to_le_bytes().map(|byte| byte.to_ascii_lowercase()));
        }
        self.find_short(key)
    }

    /// Slow path for long or non-ASCII words, and for a short word that a non-ASCII char follows.
    fn kind_long(&self, word: &str) -> Option<TokenKind> {
        if !self.case_ignored {
            return self.exact(word);
        }
        let mut buffer = [0_u8; FOLD_MAX];
        if let Some(head) = buffer.get_mut(..word.len()).filter(|_| word.is_ascii()) {
            head.copy_from_slice(word.as_bytes());
            head.make_ascii_lowercase();
            return self.exact(std::str::from_utf8(head).ok()?);
        }
        self.exact(&word.to_lowercase())
    }

    /// Looks up a word that is already folded: a short ASCII one in the table, else the map.
    fn exact(&self, word: &str) -> Option<TokenKind> {
        match Self::short_key(word) {
            Some(key) => self.find_short(key),
            None => self.long.get(word).copied(),
        }
    }
}
