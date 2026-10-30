use imara_diff::{Algorithm, Diff, InternedInput};

use super::pairing::Subhunk;

#[derive(Debug, Clone)]
pub(super) struct Pair {
    pub old: String,
    pub new: String,
}

pub(super) struct Generated {
    pub name: &'static str,
    pub pair: Pair,
    pub lines: usize,
}

pub(super) fn generated() -> Vec<Generated> {
    [(10_000, 1), (10_000, 10), (200_000, 1), (200_000, 10)]
        .into_iter()
        .map(|(lines, percent)| {
            let base = base_lines(lines);
            let old = join(&base);
            let new = edited(&base, percent, 0x5eed_0000 + percent as u64);
            Generated {
                name: match (lines, percent) {
                    (10_000, 1) => "generated-10k-1pct",
                    (10_000, 10) => "generated-10k-10pct",
                    (200_000, 1) => "generated-200k-1pct",
                    _ => "generated-200k-10pct",
                },
                pair: Pair { old, new },
                lines,
            }
        })
        .collect()
}

pub(super) fn map_pair() -> (Pair, usize) {
    let target = 50_000_000;
    let mut old = String::with_capacity(target + 1024);
    let mut index = 0;
    while old.len() < target {
        old.push_str(&map_line(index));
        old.push('\n');
        index += 1;
    }
    let mut new = String::with_capacity(old.len() + 16 * 1024);
    for (line_index, line) in old.split_inclusive('\n').enumerate() {
        let line = line.strip_suffix('\n').unwrap_or(line);
        new.push_str(line);
        if line_index % 10_000 == 5_000 {
            new.push_str(" ; revised");
        }
        new.push('\n');
    }
    (Pair { old, new }, index)
}

pub(super) fn collect_subhunks(pairs: &[Pair], max_lines: usize, limit: usize) -> Vec<Subhunk> {
    let mut subhunks = Vec::new();
    for pair in pairs {
        if subhunks.len() >= limit {
            break;
        }
        let old_lines = lines(&pair.old);
        let new_lines = lines(&pair.new);
        let input = InternedInput::new(pair.old.as_str(), pair.new.as_str());
        let diff = Diff::compute(Algorithm::Histogram, &input);
        for hunk in diff.hunks() {
            let old = slice(&old_lines, hunk.before);
            let new = slice(&new_lines, hunk.after);
            if old.is_empty() || new.is_empty() {
                continue;
            }
            if old.len() > max_lines || new.len() > max_lines {
                continue;
            }
            if old
                .iter()
                .chain(new.iter())
                .any(|line| line.chars().count() > 4096)
            {
                continue;
            }
            subhunks.push(Subhunk {
                old: old.iter().map(|line| clean(line)).collect(),
                new: new.iter().map(|line| clean(line)).collect(),
            });
            if subhunks.len() >= limit {
                break;
            }
        }
    }
    subhunks
}

pub(super) fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split_inclusive('\n').collect()
    }
}

fn slice<'a>(lines: &'a [&'a str], range: std::ops::Range<u32>) -> &'a [&'a str] {
    let start = usize::try_from(range.start).unwrap_or(usize::MAX);
    let end = usize::try_from(range.end).unwrap_or(usize::MAX);
    lines.get(start..end).unwrap_or(&[])
}

fn clean(line: &str) -> String {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line).to_owned()
}

fn base_lines(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            if index % 37 == 0 {
                format!("// section {index:06} of the generated comparison corpus")
            } else {
                format!(
                    "    let field_{index:06} = compute_{:03}(input_{:03}, {});",
                    index % 211,
                    index % 97,
                    index % 13
                )
            }
        })
        .collect()
}

fn join(lines: &[String]) -> String {
    let mut text =
        String::with_capacity(lines.iter().map(String::len).sum::<usize>() + lines.len());
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    text
}

fn edited(base: &[String], percent: usize, seed: u64) -> String {
    let count = base.len() * percent / 100;
    let indexes = selection(base.len(), count, seed);
    let mut text = String::with_capacity(base.iter().map(String::len).sum::<usize>() + base.len());
    let mut next = indexes.iter().copied().peekable();
    for (index, line) in base.iter().enumerate() {
        if next.next_if_eq(&index).is_some() {
            text.push_str(line);
            text.push_str(" // changed");
        } else {
            text.push_str(line);
        }
        text.push('\n');
    }
    text
}

fn selection(count: usize, take: usize, seed: u64) -> Vec<usize> {
    let mut indexes: Vec<usize> = (0..count).collect();
    let mut rng = Rng(seed);
    for index in (1..count).rev() {
        let swap = rng.below(index + 1);
        indexes.swap(index, swap);
    }
    indexes.truncate(take.min(count));
    indexes.sort_unstable();
    indexes
}

fn map_line(index: usize) -> String {
    match index % 10 {
        0 => format!(
            "* Section: .text.{:04} 0x{:08X}",
            index / 10 % 1000,
            0x8000_0000 + index * 16
        ),
        1 => format!(
            "  Address range 0x{:08X} - 0x{:08X}",
            0x8000_0000 + index * 16,
            0x8000_0000 + index * 16 + 16
        ),
        _ => format!(
            "  0x{:08X} 0x{:06X}  symbol_{:06}_{}  object_{:03}.o",
            0x8000_0000 + index * 16,
            16 + index % 64,
            index,
            index % 17,
            index % 113
        ),
    }
}

struct Rng(u64);

impl Rng {
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
    fn generated_edits_are_deterministic_and_exact() {
        let base = base_lines(1_000);
        let first = edited(&base, 10, 7);
        let second = edited(&base, 10, 7);
        assert_eq!(first, second);
        assert_eq!(first.lines().count(), 1_000);
        assert_eq!(
            first
                .lines()
                .filter(|line| line.ends_with("// changed"))
                .count(),
            100
        );
    }

    #[test]
    fn map_pair_is_about_fifty_megabytes_and_hundred_changed_lines() {
        let (pair, lines) = map_pair();
        assert!(pair.old.len() >= 50_000_000);
        assert_eq!(pair.old.lines().count(), lines);
        assert_eq!(
            pair.new
                .lines()
                .filter(|line| line.ends_with("; revised"))
                .count(),
            (lines + 4_999) / 10_000
        );
    }
}
