use std::io::Write;
use std::path::Path;

use super::generate::{BLOCK, INSERT_BYTES, PATCH_LEN, SEED, writer};

pub(super) struct Insertion {
    at: u64,
    bytes: Vec<u8>,
    done: bool,
}

impl Insertion {
    pub(super) fn new(at: u64, rng: &mut Rng) -> Self {
        let mut bytes = vec![0_u8; INSERT_BYTES as usize];
        rng.fill(&mut bytes);
        Self {
            at,
            bytes,
            done: false,
        }
    }

    pub(super) fn write(
        &mut self,
        output: &mut impl Write,
        block_at: u64,
        bytes: &[u8],
    ) -> Result<(), String> {
        let end = block_at + bytes.len() as u64;
        if self.done || self.at < block_at || self.at >= end {
            output.write_all(bytes).map_err(|error| error.to_string())?;
            return Ok(());
        }
        let split = usize::try_from(self.at - block_at).map_err(|_| "split")?;
        let (before, after) = bytes.split_at(split);
        output
            .write_all(before)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&self.bytes)
            .map_err(|error| error.to_string())?;
        output.write_all(after).map_err(|error| error.to_string())?;
        self.done = true;
        Ok(())
    }

    pub(super) fn finish(&mut self, output: &mut impl Write) -> Result<(), String> {
        if !self.done {
            output
                .write_all(&self.bytes)
                .map_err(|error| error.to_string())?;
            self.done = true;
        }
        Ok(())
    }
}

pub(super) fn quality_pairs() -> Vec<(String, String)> {
    [(200_000_usize, 1_usize), (200_000, 10)]
        .into_iter()
        .map(|(lines, percent)| {
            let base: Vec<String> = (0..lines)
                .map(|index| {
                    format!(
                        "    let field_{index:07} = compute_{:03}(input_{:03}, {});",
                        index % 211,
                        index % 97,
                        index % 13
                    )
                })
                .collect();
            let mut rng = Rng(SEED ^ percent as u64);
            let mut indexes: Vec<usize> = (0..lines).collect();
            for at in (1..lines).rev() {
                let swap = rng.below(at + 1);
                indexes.swap(at, swap);
            }
            indexes.truncate(lines * percent / 100);
            indexes.sort_unstable();
            let mut changed = String::with_capacity(base.len() * 64);
            let mut next = indexes.into_iter().peekable();
            for (index, line) in base.iter().enumerate() {
                changed.push_str(line);
                if next.next_if_eq(&index).is_some() {
                    changed.push_str(" // changed");
                }
                changed.push('\n');
            }
            (format!("{lines}-{}pct", percent), changed)
        })
        .collect()
}

pub(super) struct ContentSpec {
    pub size: u64,
    pub seed: u64,
    pub differing: bool,
}

pub(super) fn write_content_pair(
    old_path: &Path,
    new_path: &Path,
    spec: &ContentSpec,
) -> Result<(), String> {
    let mut old = writer(old_path)?;
    let mut new = writer(new_path)?;
    let mut buffer = vec![0_u8; BLOCK];
    let mut rng = Rng(SEED ^ spec.seed ^ spec.size);
    let mut at = 0_u64;
    while at < spec.size {
        let want = usize::try_from((spec.size - at).min(BLOCK as u64)).map_err(|_| "block size")?;
        let bytes = buffer.get_mut(..want).ok_or("short block")?;
        rng.fill(bytes);
        old.write_all(bytes)
            .map_err(|error| format!("{}: {error}", old_path.display()))?;
        if spec.differing
            && at + want as u64 == spec.size
            && let Some(last) = bytes.last_mut()
        {
            *last = last.wrapping_add(1);
        }
        new.write_all(bytes)
            .map_err(|error| format!("{}: {error}", new_path.display()))?;
        at += want as u64;
    }
    old.flush()
        .map_err(|error| format!("{}: {error}", old_path.display()))?;
    new.flush()
        .map_err(|error| format!("{}: {error}", new_path.display()))?;
    Ok(())
}

pub(super) struct LogLine {
    id: u64,
    shift: bool,
    changed: bool,
    inserted: bool,
}

impl LogLine {
    pub(super) fn new(id: u64, shift: bool, changed: bool, inserted: bool) -> Self {
        Self {
            id,
            shift,
            changed,
            inserted,
        }
    }
}

pub(super) fn format_log_line(line: LogLine, output: &mut Vec<u8>) {
    output.clear();
    let id = line.id;
    let seconds = id + u64::from(line.shift);
    let hour = seconds / 3600 % 24;
    let minute = seconds / 60 % 60;
    let second = seconds % 60;
    let millis = id * 37 % 1000;
    let worker = id % 8;
    let kind = if line.inserted { "batch" } else { "request" };
    let suffix = if line.changed { " changed" } else { "" };
    use std::io::Write as _;
    if write!(
        output,
        "2026-10-20 {hour:02}:{minute:02}:{second:02}.{millis:03} INFO  worker-{worker:02} id={id:010} path=/var/log/ferrite/service.log msg={kind} completed in {} ms{suffix}",
        id % 97
    )
    .is_err()
    {
        output.clear();
    }
}

pub(super) fn apply_patches(buffer: &mut [u8], at: u64, patches: &[u64]) {
    for &patch in patches {
        let start = patch.max(at);
        let end = (patch + PATCH_LEN).min(at + buffer.len() as u64);
        if start >= end {
            continue;
        }
        let Ok(from) = usize::try_from(start - at) else {
            continue;
        };
        let Ok(to) = usize::try_from(end - at) else {
            continue;
        };
        for (offset, byte) in buffer[from..to].iter_mut().enumerate() {
            *byte = (*byte).wrapping_add(0x5a).wrapping_add(offset as u8);
        }
    }
}

pub(super) struct Rng(pub(super) u64);

impl Rng {
    pub(super) fn fill(&mut self, bytes: &mut [u8]) {
        for chunk in bytes.chunks_mut(8) {
            let value = self.next().to_le_bytes();
            let count = chunk.len();
            if let Some(target) = chunk.get_mut(..count) {
                target.copy_from_slice(&value[..count]);
            }
        }
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            usize::try_from(self.next() % bound as u64).unwrap_or(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_edits_are_exact() {
        for (name, changed) in quality_pairs() {
            let lines = name
                .split('-')
                .next()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            assert_eq!(changed.lines().count(), lines);
            let percent = name
                .split('-')
                .nth(1)
                .and_then(|value| value.trim_end_matches("pct").parse::<usize>().ok())
                .unwrap_or(0);
            assert_eq!(
                changed
                    .lines()
                    .filter(|line| line.ends_with("// changed"))
                    .count(),
                lines * percent / 100
            );
        }
    }
}
