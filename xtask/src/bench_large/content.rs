use std::fs::File;
use std::time::Instant;

use blake3::Hasher;
use rusty_core::paging::ByteSource;

use super::generate::Pair;
use super::scan::{BLOCK, FileSource, map_file, read_file_at};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Buffered,
    Mapped,
}

pub struct SetMeasure {
    pub buffered: Vec<f64>,
    pub mapped: Vec<f64>,
    pub pairs: u64,
    pub equal: u64,
    pub first_difference: Option<u64>,
    pub mapped_peak_bytes: usize,
}

pub struct FolderReport {
    pub large: SetMeasure,
    pub small: SetMeasure,
}

struct Check {
    equal: bool,
    first_difference: Option<u64>,
}

pub fn measure(large: &[Pair], small: &[Pair], runs: usize) -> Result<FolderReport, String> {
    let large_measure = measure_set(large, runs)?;
    let small_measure = measure_set(small, runs)?;
    Ok(FolderReport {
        large: large_measure,
        small: small_measure,
    })
}

fn measure_set(pairs: &[Pair], runs: usize) -> Result<SetMeasure, String> {
    let mut buffered = Vec::with_capacity(runs);
    let mut mapped = Vec::with_capacity(runs);
    let mut equal = 0_u64;
    let mut first_difference = None;
    let mut mapped_peak = 0_usize;
    for _ in 0..runs {
        let started = Instant::now();
        for pair in pairs {
            let check = check_pair(pair, Mode::Buffered)?;
            equal += u64::from(check.equal);
            first_difference = first_difference.or(check.first_difference);
        }
        buffered.push(elapsed_ms(started));
        let started = Instant::now();
        for pair in pairs {
            let check = check_pair(pair, Mode::Mapped)?;
            equal += u64::from(check.equal);
            first_difference = first_difference.or(check.first_difference);
            mapped_peak = mapped_peak.max(pair_size(pair));
        }
        mapped.push(elapsed_ms(started));
    }
    Ok(SetMeasure {
        buffered,
        mapped,
        pairs: pairs.len() as u64,
        equal,
        first_difference,
        mapped_peak_bytes: mapped_peak,
    })
}

fn pair_size(pair: &Pair) -> usize {
    let bytes = pair.old_bytes.max(pair.new_bytes);
    usize::try_from(bytes.saturating_mul(2)).unwrap_or(usize::MAX)
}

fn check_pair(pair: &Pair, mode: Mode) -> Result<Check, String> {
    match mode {
        Mode::Buffered => check_buffered(pair),
        Mode::Mapped => check_mapped(pair),
    }
}

fn check_buffered(pair: &Pair) -> Result<Check, String> {
    let old = FileSource::open(&pair.old)?;
    let new = FileSource::open(&pair.new)?;
    let len = old.len();
    if len != new.len() {
        return Ok(Check {
            equal: false,
            first_difference: Some(len.min(new.len())),
        });
    }
    let mut old_buffer = vec![0_u8; BLOCK];
    let mut new_buffer = vec![0_u8; BLOCK];
    let mut old_hash = Hasher::new();
    let mut new_hash = Hasher::new();
    let mut at = 0_u64;
    while at < len {
        let want = usize::try_from((len - at).min(BLOCK as u64)).map_err(|_| "block size")?;
        let read = read_file_at(old.file(), at, &mut old_buffer[..want])?;
        let other = read_file_at(new.file(), at, &mut new_buffer[..want])?;
        let count = read.min(other);
        if count == 0 {
            break;
        }
        if old_buffer[..count] != new_buffer[..count] {
            let within = first_difference(&old_buffer[..count], &new_buffer[..count]);
            return Ok(Check {
                equal: false,
                first_difference: within.map(|within| at + within as u64),
            });
        }
        old_hash.update(&old_buffer[..count]);
        new_hash.update(&new_buffer[..count]);
        at += count as u64;
    }
    Ok(Check {
        equal: old_hash.finalize() == new_hash.finalize(),
        first_difference: None,
    })
}

fn check_mapped(pair: &Pair) -> Result<Check, String> {
    let old = File::open(&pair.old).map_err(|error| format!("{}: {error}", pair.old.display()))?;
    let new = File::open(&pair.new).map_err(|error| format!("{}: {error}", pair.new.display()))?;
    let old_map = map_file(&old)?;
    let new_map = map_file(&new)?;
    if old_map.len() != new_map.len() {
        return Ok(Check {
            equal: false,
            first_difference: Some(old_map.len().min(new_map.len()) as u64),
        });
    }
    let mut old_hash = Hasher::new();
    let mut new_hash = Hasher::new();
    let mut at = 0;
    while at < old_map.len() {
        let end = (at + BLOCK).min(old_map.len());
        let left = &old_map[at..end];
        let right = &new_map[at..end];
        if left != right {
            let within = first_difference(left, right);
            return Ok(Check {
                equal: false,
                first_difference: within.map(|within| (at + within) as u64),
            });
        }
        old_hash.update(left);
        new_hash.update(right);
        at = end;
    }
    Ok(Check {
        equal: old_hash.finalize() == new_hash.finalize(),
        first_difference: None,
    })
}

fn first_difference(old: &[u8], new: &[u8]) -> Option<usize> {
    old.iter().zip(new).position(|(left, right)| left != right)
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_difference_finds_the_first_byte() {
        assert_eq!(first_difference(b"abc", b"abx"), Some(2));
        assert_eq!(first_difference(b"abc", b"abc"), None);
    }
}
