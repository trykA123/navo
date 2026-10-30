use std::cmp::Ordering;
use std::fs::File;
use std::path::Path;

use blake3::Hasher;
use fastcdc::v2020::StreamCDC;
use rusty_core::paging::ByteSource;
use similar::utils::diff_slices;
use similar::{Algorithm, ChangeTag};

use super::scan::{BLOCK, FileSource, read_file_at};

const MIN_CHUNK: usize = 2 * 1024;
const AVG_CHUNK: usize = 8 * 1024;
const MAX_CHUNK: usize = 64 * 1024;
const MERGE_GAP: u64 = 16;
const REFINE_LIMIT: u64 = 64 * 1024;

pub struct OffsetResult {
    pub regions: u64,
    pub changed_bytes: u64,
    pub first_difference: Option<u64>,
    pub tracked_bytes: usize,
}

pub struct ContentResult {
    pub regions: u64,
    pub inserted_bytes: u64,
    pub refined: u64,
    pub first_difference: Option<u64>,
    pub tracked_bytes: usize,
}

pub fn by_offset(old: &Path, new: &Path) -> Result<OffsetResult, String> {
    let old_file = FileSource::open(old)?;
    let new_file = FileSource::open(new)?;
    let len = old_file.len();
    if len != new_file.len() {
        return Err("offset alignment needs equal sizes".to_owned());
    }
    let mut old_buffer = vec![0_u8; BLOCK];
    let mut new_buffer = vec![0_u8; BLOCK];
    let mut at = 0_u64;
    let mut current: Option<(u64, u64)> = None;
    let mut regions = 0_u64;
    let mut changed_bytes = 0_u64;
    let mut first_difference = None;
    while at < len {
        let want = usize::try_from((len - at).min(BLOCK as u64)).map_err(|_| "block size")?;
        let old_read = read_file_at(old_file.file(), at, &mut old_buffer[..want])?;
        let new_read = read_file_at(new_file.file(), at, &mut new_buffer[..want])?;
        let read = old_read.min(new_read);
        if read == 0 {
            break;
        }
        for (index, (left, right)) in old_buffer[..read]
            .iter()
            .zip(&new_buffer[..read])
            .enumerate()
        {
            if left == right {
                continue;
            }
            let offset = at + index as u64;
            first_difference.get_or_insert(offset);
            changed_bytes += 1;
            note_difference(offset, &mut current, &mut regions);
        }
        at += read as u64;
    }
    if current.is_some() {
        regions += 1;
    }
    Ok(OffsetResult {
        regions,
        changed_bytes,
        first_difference,
        tracked_bytes: 2 * BLOCK,
    })
}

fn note_difference(offset: u64, current: &mut Option<(u64, u64)>, regions: &mut u64) {
    match *current {
        Some((start, end)) if offset <= end + MERGE_GAP => {
            *current = Some((start, offset + 1));
        }
        Some(_) => {
            *regions += 1;
            *current = Some((offset, offset + 1));
        }
        None => *current = Some((offset, offset + 1)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Chunk {
    hash: [u8; 32],
    offset: u64,
    length: u32,
}

#[derive(Clone, Copy)]
struct Match {
    old: u64,
    new: u64,
    length: u32,
}

pub fn by_content(old: &Path, new: &Path) -> Result<ContentResult, String> {
    let old_chunks = chunk_file(old)?;
    let new_chunks = chunk_file(new)?;
    let mut matches = unique_chunks(&old_chunks, &new_chunks);
    matches.sort_unstable_by_key(|found| found.old);
    let tracks = old_chunks.len() * 48 + new_chunks.len() * 48 + matches.len() * 16;
    let anchors = longest_increasing(&matches);
    let sides = Sides {
        old: FileSource::open(old)?,
        new: FileSource::open(new)?,
    };
    let mut scan = ContentScan::new(&sides);
    for anchor in &anchors {
        scan.gap(anchor.old, anchor.new)?;
        scan.old_at = anchor.old + u64::from(anchor.length);
        scan.new_at = anchor.new + u64::from(anchor.length);
    }
    scan.gap(sides.old.len(), sides.new.len())?;
    Ok(scan.finish(tracks))
}

struct Sides {
    old: FileSource,
    new: FileSource,
}

struct ContentScan<'a> {
    sides: &'a Sides,
    regions: u64,
    inserted_bytes: u64,
    refined: u64,
    first_difference: Option<u64>,
    old_at: u64,
    new_at: u64,
}

impl<'a> ContentScan<'a> {
    fn new(sides: &'a Sides) -> Self {
        Self {
            sides,
            regions: 0,
            inserted_bytes: 0,
            refined: 0,
            first_difference: None,
            old_at: 0,
            new_at: 0,
        }
    }

    fn gap(&mut self, old_end: u64, new_end: u64) -> Result<(), String> {
        if old_end <= self.old_at && new_end <= self.new_at {
            return Ok(());
        }
        self.regions += 1;
        self.inserted_bytes += new_end.saturating_sub(self.new_at);
        self.first_difference.get_or_insert(self.old_at);
        if old_end - self.old_at <= REFINE_LIMIT && new_end - self.new_at <= REFINE_LIMIT {
            refine(
                self.sides,
                self.old_at..old_end,
                self.new_at..new_end,
                &mut self.refined,
            )?;
        }
        Ok(())
    }

    fn finish(self, tracked: usize) -> ContentResult {
        ContentResult {
            regions: self.regions,
            inserted_bytes: self.inserted_bytes,
            refined: self.refined,
            first_difference: self.first_difference,
            tracked_bytes: tracked,
        }
    }
}

fn refine(
    sides: &Sides,
    old: std::ops::Range<u64>,
    new: std::ops::Range<u64>,
    refined: &mut u64,
) -> Result<(), String> {
    let old_bytes = read_range(&sides.old, old.start, old.end)?;
    let new_bytes = read_range(&sides.new, new.start, new.end)?;
    for (tag, slice) in diff_slices(Algorithm::Myers, &old_bytes, &new_bytes) {
        if tag != ChangeTag::Equal {
            *refined += slice.len() as u64;
        }
    }
    Ok(())
}

fn read_range(source: &FileSource, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let len = usize::try_from(end.saturating_sub(start)).map_err(|_| "range is too large")?;
    let mut bytes = vec![0_u8; len];
    let read = read_file_at(source.file(), start, &mut bytes)?;
    if read != len {
        return Err(format!("short read at {start}: {read} of {len}"));
    }
    Ok(bytes)
}

fn chunk_file(path: &Path) -> Result<Vec<Chunk>, String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let chunker = StreamCDC::new(file, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK);
    let mut chunks = Vec::new();
    for result in chunker {
        let chunk = result.map_err(|error| format!("{}: {error}", path.display()))?;
        let length = u32::try_from(chunk.length).map_err(|_| "chunk is too large")?;
        chunks.push(Chunk {
            hash: *Hasher::new().update(&chunk.data).finalize().as_bytes(),
            offset: chunk.offset,
            length,
        });
    }
    Ok(chunks)
}

fn unique_chunks(old: &[Chunk], new: &[Chunk]) -> Vec<Match> {
    let mut old_sorted: Vec<&Chunk> = old.iter().collect();
    let mut new_sorted: Vec<&Chunk> = new.iter().collect();
    old_sorted.sort_unstable_by_key(|chunk| chunk.hash);
    new_sorted.sort_unstable_by_key(|chunk| chunk.hash);
    let mut matches = Vec::new();
    let mut left = 0;
    let mut right = 0;
    while left < old_sorted.len() && right < new_sorted.len() {
        match old_sorted[left].hash.cmp(&new_sorted[right].hash) {
            Ordering::Less => left += 1,
            Ordering::Greater => right += 1,
            Ordering::Equal => {
                let old_end = chunk_run_end(&old_sorted, left);
                let new_end = chunk_run_end(&new_sorted, right);
                if old_end - left == 1 && new_end - right == 1 {
                    matches.push(Match {
                        old: old_sorted[left].offset,
                        new: new_sorted[right].offset,
                        length: old_sorted[left].length,
                    });
                }
                left = old_end;
                right = new_end;
            }
        }
    }
    matches
}

fn chunk_run_end(chunks: &[&Chunk], start: usize) -> usize {
    let mut end = start + 1;
    while end < chunks.len() && chunks[end].hash == chunks[start].hash {
        end += 1;
    }
    end
}

fn longest_increasing(matches: &[Match]) -> Vec<Match> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![usize::MAX; matches.len()];
    for (index, found) in matches.iter().enumerate() {
        let at = tails.partition_point(|&tail| matches[tail].new < found.new);
        if let Some(&before) = at.checked_sub(1).and_then(|before| tails.get(before)) {
            previous[index] = before;
        }
        if at == tails.len() {
            tails.push(index);
        } else if let Some(slot) = tails.get_mut(at) {
            *slot = index;
        }
    }
    let mut anchors = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(index) = at {
        if let Some(found) = matches.get(index) {
            anchors.push(*found);
        }
        at = previous
            .get(index)
            .copied()
            .filter(|value| *value != usize::MAX);
    }
    anchors.reverse();
    anchors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_merges_runs_closer_than_sixteen_bytes() {
        let root = std::env::temp_dir().join("rusty-bench-large-offset-test");
        let old = root.join("old");
        let new = root.join("new");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&old, vec![0_u8; 128]).unwrap();
        let mut changed = vec![0_u8; 128];
        changed[10] = 1;
        changed[20] = 1;
        changed[100] = 1;
        std::fs::write(&new, &changed).unwrap();
        let result = by_offset(&old, &new).unwrap();
        assert_eq!(result.regions, 2);
        assert_eq!(result.changed_bytes, 3);
        let _ = std::fs::remove_dir_all(&root);
    }
}
