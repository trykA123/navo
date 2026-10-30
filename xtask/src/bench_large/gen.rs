use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::patterns::{
    ContentSpec, Insertion, LogLine, Rng, apply_patches, format_log_line, write_content_pair,
};

pub(super) const BLOCK: usize = 1024 * 1024;
pub(super) const SEED: u64 = 0x5eed_0000_0000_0042;
const INSERT_LINES: u64 = 50_000;
pub(super) const PATCH_LEN: u64 = 64;
pub(super) const INSERT_BYTES: u64 = 4096;

pub struct Corpus {
    root: PathBuf,
}

impl Corpus {
    pub fn prepare(root: PathBuf) -> Result<Self, String> {
        if let Err(error) = fs::remove_dir_all(&root)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!("{}: {error}", root.display()));
        }
        fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
        Ok(Self { root })
    }

    pub fn dir(&self, name: &str) -> Result<PathBuf, String> {
        let path = self.root.join(name);
        fs::create_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(path)
    }

    pub fn remove(&self, path: &Path) -> Result<(), String> {
        fs::remove_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "bench-large: cannot remove {}: {error}",
                self.root.display()
            );
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pair {
    pub old: PathBuf,
    pub new: PathBuf,
    pub old_bytes: u64,
    pub new_bytes: u64,
}

pub struct LogPair {
    pub pair: Pair,
}

pub struct BinaryPair {
    pub pair: Pair,
}

pub fn log_pair(
    corpus: &Corpus,
    name: &str,
    target: u64,
    all_timestamps_differ: bool,
) -> Result<LogPair, String> {
    let dir = corpus.dir(name)?;
    let old_path = dir.join("old.log");
    let new_path = dir.join("new.log");
    let mut old = writer(&old_path)?;
    let mut new = writer(&new_path)?;
    let insert_at = target * 2 / 5;
    let delete_at = target * 7 / 10;
    let mut old_bytes = 0_u64;
    let mut new_bytes = 0_u64;
    let mut line = 0_u64;
    let mut inserted = false;
    let mut deletion_started = false;
    let mut deleted = 0_u64;
    let mut buffer = Vec::with_capacity(160);
    while old_bytes < target {
        if !inserted && old_bytes >= insert_at {
            new_bytes += write_inserted_log(&mut new, &mut buffer)?;
            inserted = true;
        }
        if !deletion_started && old_bytes >= delete_at {
            deletion_started = true;
            deleted = INSERT_LINES;
        }
        let changed = line.is_multiple_of(1000);
        format_log_line(LogLine::new(line, false, false, false), &mut buffer);
        old_bytes += write_line(&mut old, &buffer)?;
        if deleted > 0 {
            deleted -= 1;
        } else {
            format_log_line(
                LogLine::new(line, all_timestamps_differ, changed, false),
                &mut buffer,
            );
            new_bytes += write_line(&mut new, &buffer)?;
        }
        line += 1;
    }
    old.flush()
        .map_err(|error| format!("{}: {error}", old_path.display()))?;
    new.flush()
        .map_err(|error| format!("{}: {error}", new_path.display()))?;
    Ok(LogPair {
        pair: Pair {
            old: old_path,
            new: new_path,
            old_bytes,
            new_bytes,
        },
    })
}

fn write_inserted_log(output: &mut impl Write, buffer: &mut Vec<u8>) -> Result<u64, String> {
    let mut bytes = 0;
    for index in 0..INSERT_LINES {
        let id = 1_000_000_000 + index;
        format_log_line(LogLine::new(id, false, false, true), buffer);
        bytes += write_line(output, buffer)?;
    }
    Ok(bytes)
}

pub fn binary_patched(corpus: &Corpus, name: &str, size: u64) -> Result<BinaryPair, String> {
    let patches: Vec<u64> = (0..1000)
        .map(|index| size / 1000 * index + size / 2000)
        .collect();
    let dir = corpus.dir(name)?;
    let old_path = dir.join("old.bin");
    let new_path = dir.join("new.bin");
    let mut old = writer(&old_path)?;
    let mut new = writer(&new_path)?;
    let mut rng = Rng(SEED ^ size);
    let mut old_buffer = vec![0_u8; BLOCK];
    let mut new_buffer = vec![0_u8; BLOCK];
    let mut at = 0_u64;
    while at < size {
        let want =
            usize::try_from((size - at).min(BLOCK as u64)).map_err(|_| "block size".to_owned())?;
        let bytes = old_buffer.get_mut(..want).ok_or("short block")?;
        rng.fill(bytes);
        old.write_all(bytes)
            .map_err(|error| format!("{}: {error}", old_path.display()))?;
        let copy = new_buffer.get_mut(..want).ok_or("short block")?;
        copy.copy_from_slice(bytes);
        apply_patches(copy, at, &patches);
        new.write_all(copy)
            .map_err(|error| format!("{}: {error}", new_path.display()))?;
        at += want as u64;
    }
    old.flush()
        .map_err(|error| format!("{}: {error}", old_path.display()))?;
    new.flush()
        .map_err(|error| format!("{}: {error}", new_path.display()))?;
    Ok(BinaryPair {
        pair: Pair {
            old: old_path,
            new: new_path,
            old_bytes: size,
            new_bytes: size,
        },
    })
}

pub fn binary_inserted(corpus: &Corpus, name: &str, size: u64) -> Result<BinaryPair, String> {
    let dir = corpus.dir(name)?;
    let old_path = dir.join("old.bin");
    let new_path = dir.join("new.bin");
    let mut old = writer(&old_path)?;
    let mut new = writer(&new_path)?;
    let mut rng = Rng(SEED ^ (size << 1));
    let mut old_buffer = vec![0_u8; BLOCK];
    let mut insertion = Insertion::new(size * 3 / 10, &mut rng);
    let mut at = 0_u64;
    while at < size {
        let want =
            usize::try_from((size - at).min(BLOCK as u64)).map_err(|_| "block size".to_owned())?;
        let bytes = old_buffer.get_mut(..want).ok_or("short block")?;
        rng.fill(bytes);
        old.write_all(bytes)
            .map_err(|error| format!("{}: {error}", old_path.display()))?;
        insertion.write(&mut new, at, bytes)?;
        at += want as u64;
    }
    insertion.finish(&mut new)?;
    old.flush()
        .map_err(|error| format!("{}: {error}", old_path.display()))?;
    new.flush()
        .map_err(|error| format!("{}: {error}", new_path.display()))?;
    Ok(BinaryPair {
        pair: Pair {
            old: old_path,
            new: new_path,
            old_bytes: size,
            new_bytes: size + INSERT_BYTES,
        },
    })
}

pub fn content_pairs(
    corpus: &Corpus,
    name: &str,
    count: u64,
    size: u64,
) -> Result<Vec<Pair>, String> {
    let dir = corpus.dir(name)?;
    let mut pairs = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for index in 0..count {
        let old_path = dir.join(format!("pair-{index:05}.old"));
        let new_path = dir.join(format!("pair-{index:05}.new"));
        let spec = ContentSpec {
            size,
            seed: index,
            differing: index + 1 == count,
        };
        write_content_pair(&old_path, &new_path, &spec)?;
        pairs.push(Pair {
            old: old_path,
            new: new_path,
            old_bytes: size,
            new_bytes: size,
        });
    }
    Ok(pairs)
}

pub(super) fn writer(path: &Path) -> Result<BufWriter<File>, String> {
    let file = File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(BufWriter::with_capacity(BLOCK, file))
}

fn write_line(output: &mut impl Write, line: &[u8]) -> Result<u64, String> {
    output.write_all(line).map_err(|error| error.to_string())?;
    output.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok((line.len() + 1) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_pairs_are_deterministic_and_clean_up() {
        let root = std::env::temp_dir().join("rusty-bench-large-gen-test");
        {
            let corpus = Corpus::prepare(root.clone()).unwrap();
            let first = log_pair(&corpus, "log", 2 * BLOCK as u64, false).unwrap();
            let second = log_pair(&corpus, "log2", 2 * BLOCK as u64, false).unwrap();
            assert_eq!(first.pair.old_bytes, second.pair.old_bytes);
            assert!(first.pair.old_bytes >= 2 * BLOCK as u64);
            assert!(first.pair.new_bytes > 0);
        }
        assert!(!root.exists());
    }
}
