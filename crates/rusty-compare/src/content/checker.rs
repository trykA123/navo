use std::io::Read;

use blake3::Hasher;

use crate::model::{
    CancelToken, ContentClass, ContentKey, HashCache, ReadHint, Reason, Stamp, Status,
};

use super::read::{
    changed, check_kinds, check_object_ids, class, content_key, different, different_class,
    finish_side, first_difference, open_pair, outcome, read_block, read_pair, stamp_matches,
    stopped, unavailable,
};
use super::{CONTENT_BLOCK, CheckOptions, ContentProgress, ContentResult, Side};

type CachedHashes = (Option<[u8; 32]>, Option<[u8; 32]>);

#[derive(Clone, Copy)]
struct Sides<'a> {
    left: Side<'a>,
    right: Side<'a>,
}

#[derive(Clone, Copy)]
struct Cached<'a> {
    key: &'a ContentKey,
    hash: [u8; 32],
}

#[derive(Clone, Copy)]
struct Counts {
    done: u64,
    total: u64,
}

impl Counts {
    const fn new(total: u64) -> Self {
        Self { done: 0, total }
    }

    fn add(&mut self, read: usize, block: usize) -> bool {
        self.done = self
            .done
            .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        self.done > self.total || (read < block && self.done != self.total)
    }
}

struct Stream {
    left_reader: Box<dyn Read + Send>,
    right_reader: Box<dyn Read + Send>,
    left_buffer: Vec<u8>,
    right_buffer: Vec<u8>,
    left_counts: Counts,
    right_counts: Counts,
}

struct Step {
    left: usize,
    right: usize,
    left_eof: bool,
    right_eof: bool,
}

impl Stream {
    fn open(sides: Sides<'_>, hint: ReadHint) -> Result<Self, ContentResult> {
        let (left_reader, right_reader) = open_pair(sides.left, sides.right, hint)?;
        Ok(Self {
            left_reader,
            right_reader,
            left_buffer: vec![0u8; CONTENT_BLOCK],
            right_buffer: vec![0u8; CONTENT_BLOCK],
            left_counts: Counts::new(sides.left.entry.size),
            right_counts: Counts::new(sides.right.entry.size),
        })
    }

    fn step(&mut self) -> Result<Step, ContentResult> {
        let (left, right) = read_pair(
            &mut *self.left_reader,
            &mut *self.right_reader,
            &mut self.left_buffer,
            &mut self.right_buffer,
        )?;
        let left_changed = self.left_counts.add(left, self.left_buffer.len());
        let right_changed = self.right_counts.add(right, self.right_buffer.len());
        if left_changed || right_changed {
            return Err(changed());
        }
        Ok(Step {
            left,
            right,
            left_eof: left < self.left_buffer.len(),
            right_eof: right < self.right_buffer.len(),
        })
    }
}

#[derive(Clone, Copy)]
struct Block<'a> {
    offset: u64,
    left: &'a [u8],
    right: &'a [u8],
    saw_nul: bool,
}

#[derive(Clone, Copy)]
struct Digest {
    hash: [u8; 32],
    saw_nul: bool,
}

#[derive(Clone, Copy)]
struct Completion<'a> {
    keys: (&'a ContentKey, &'a ContentKey),
    hashers: (&'a Hasher, &'a Hasher),
    saw_nul: bool,
}

pub fn check(
    left: Side<'_>,
    right: Side<'_>,
    options: &CheckOptions<'_>,
    progress: &mut dyn FnMut(ContentProgress),
) -> ContentResult {
    let mut checker = Checker {
        cache: options.cache,
        cancel: options.cancel,
        read_hint: options.read_hint,
        known_same: options.known_same,
        left_stamp: Stamp::default(),
        right_stamp: Stamp::default(),
        progress,
    };
    checker.run(Sides { left, right })
}

struct Checker<'a, 'b> {
    cache: &'a dyn HashCache,
    cancel: &'a CancelToken,
    read_hint: ReadHint,
    known_same: bool,
    left_stamp: Stamp,
    right_stamp: Stamp,
    progress: &'b mut dyn FnMut(ContentProgress),
}

impl Checker<'_, '_> {
    fn run(&mut self, sides: Sides<'_>) -> ContentResult {
        let left = sides.left;
        let right = sides.right;
        if self.cancel.is_cancelled() {
            return stopped();
        }
        if let Some(result) = check_kinds(left.entry, right.entry) {
            return result;
        }
        if self.known_same {
            return outcome(Status::Same, ContentClass::Unknown);
        }
        if let Some(result) = check_object_ids(left, right) {
            return result;
        }
        if left.entry.size != right.entry.size {
            return different(None, ContentClass::Unknown);
        }

        let left_key = content_key(left.source, left.entry);
        let right_key = content_key(right.source, right.entry);
        let keys = (&left_key, &right_key);
        let (left_hash, right_hash) = match self.cache_hashes(sides, keys) {
            Ok(hashes) => hashes,
            Err(result) => return result,
        };
        match (left_hash, right_hash) {
            (Some(left_hash), Some(right_hash)) if left_hash == right_hash => {
                outcome(Status::Same, ContentClass::Unknown)
            }
            (Some(_), Some(_)) => different(None, ContentClass::Unknown),
            (Some(cached), None) => self.one_side(
                sides,
                right,
                Cached {
                    key: &right_key,
                    hash: cached,
                },
            ),
            (None, Some(cached)) => self.one_side(
                sides,
                left,
                Cached {
                    key: &left_key,
                    hash: cached,
                },
            ),
            (None, None) => self.both(sides, keys),
        }
    }

    fn cache_hashes(
        &mut self,
        sides: Sides<'_>,
        keys: (&ContentKey, &ContentKey),
    ) -> Result<CachedHashes, ContentResult> {
        let left_stamp = sides
            .left
            .source
            .stamp(&sides.left.entry.rel)
            .map_err(unavailable)?;
        let right_stamp = sides
            .right
            .source
            .stamp(&sides.right.entry.rel)
            .map_err(unavailable)?;
        if !stamp_matches(&left_stamp, sides.left.entry)
            || !stamp_matches(&right_stamp, sides.right.entry)
        {
            return Err(changed());
        }
        self.left_stamp = left_stamp;
        self.right_stamp = right_stamp;
        Ok((
            self.cached(&left_stamp, keys.0),
            self.cached(&right_stamp, keys.1),
        ))
    }

    fn cached(&self, stamp: &Stamp, key: &ContentKey) -> Option<[u8; 32]> {
        if stamp.file_id == key.file_id {
            self.cache.get(key)
        } else {
            None
        }
    }

    fn one_side(&mut self, sides: Sides<'_>, read: Side<'_>, cached: Cached<'_>) -> ContentResult {
        let mut reader = match read.source.open(&read.entry.rel, self.read_hint) {
            Ok(reader) => reader,
            Err(reason) => return unavailable(reason),
        };
        let mut counts = Counts::new(read.entry.size);
        let mut hasher = Hasher::new();
        let mut saw_nul = false;
        let mut buffer = vec![0u8; CONTENT_BLOCK];
        loop {
            if self.cancel.is_cancelled() {
                return stopped();
            }
            let read_bytes = match read_block(&mut *reader, &mut buffer) {
                Ok(read) => read,
                Err(_) => return unavailable(Reason::ReadError),
            };
            if counts.add(read_bytes, buffer.len()) {
                return changed();
            }
            if read_bytes == 0 {
                let digest = Digest {
                    hash: *hasher.finalize().as_bytes(),
                    saw_nul,
                };
                return self.finish_one(sides, cached, digest);
            }
            saw_nul |= buffer[..read_bytes].contains(&0);
            hasher.update(&buffer[..read_bytes]);
            self.report(counts.done, counts.total);
        }
    }

    fn both(&mut self, sides: Sides<'_>, keys: (&ContentKey, &ContentKey)) -> ContentResult {
        let mut stream = match Stream::open(sides, self.read_hint) {
            Ok(stream) => stream,
            Err(result) => return result,
        };
        let mut left_hasher = Hasher::new();
        let mut right_hasher = Hasher::new();
        let mut saw_nul = false;
        let mut offset = 0u64;
        loop {
            if self.cancel.is_cancelled() {
                return stopped();
            }
            let step = match stream.step() {
                Ok(step) => step,
                Err(result) => return result,
            };
            saw_nul |= stream.left_buffer[..step.left].contains(&0)
                || stream.right_buffer[..step.right].contains(&0);
            left_hasher.update(&stream.left_buffer[..step.left]);
            right_hasher.update(&stream.right_buffer[..step.right]);
            let block = step.left.max(step.right);
            let done = offset.saturating_add(u64::try_from(block).unwrap_or(u64::MAX));
            self.report(done, stream.left_counts.total);
            if step.left != step.right
                || stream.left_buffer[..step.left] != stream.right_buffer[..step.right]
            {
                let block = Block {
                    offset,
                    left: &stream.left_buffer[..step.left],
                    right: &stream.right_buffer[..step.right],
                    saw_nul,
                };
                return self.block_difference(sides, block);
            }
            offset = offset.saturating_add(u64::try_from(step.left).unwrap_or(u64::MAX));
            if step.left_eof && step.right_eof {
                let completion = Completion {
                    keys,
                    hashers: (&left_hasher, &right_hasher),
                    saw_nul,
                };
                return self.finish_both(sides, completion);
            }
        }
    }

    fn block_difference(&self, sides: Sides<'_>, block: Block<'_>) -> ContentResult {
        let within = first_difference(block.left, block.right);
        let first = block
            .offset
            .saturating_add(u64::try_from(within).unwrap_or(u64::MAX));
        match self.unchanged(sides) {
            Ok(()) => different(Some(first), different_class(block.saw_nul)),
            Err(result) => result,
        }
    }

    fn finish_one(&self, sides: Sides<'_>, cached: Cached<'_>, digest: Digest) -> ContentResult {
        match self.unchanged(sides) {
            Ok(()) => finish_side(
                self.cache,
                cached.key,
                (digest.hash, cached.hash),
                digest.saw_nul,
            ),
            Err(result) => result,
        }
    }

    fn finish_both(&self, sides: Sides<'_>, completion: Completion<'_>) -> ContentResult {
        match self.unchanged(sides) {
            Ok(()) => self.finish_same(completion.keys, completion.hashers, completion.saw_nul),
            Err(result) => result,
        }
    }

    fn finish_same(
        &self,
        keys: (&ContentKey, &ContentKey),
        hashers: (&Hasher, &Hasher),
        saw_nul: bool,
    ) -> ContentResult {
        self.cache.put(keys.0, *hashers.0.finalize().as_bytes());
        self.cache.put(keys.1, *hashers.1.finalize().as_bytes());
        outcome(Status::Same, class(saw_nul))
    }

    fn unchanged(&self, sides: Sides<'_>) -> Result<(), ContentResult> {
        let left = sides
            .left
            .source
            .stamp(&sides.left.entry.rel)
            .map_err(unavailable)?;
        let right = sides
            .right
            .source
            .stamp(&sides.right.entry.rel)
            .map_err(unavailable)?;
        if left == self.left_stamp && right == self.right_stamp {
            Ok(())
        } else {
            Err(changed())
        }
    }

    fn report(&mut self, done: u64, total: u64) {
        (self.progress)(ContentProgress {
            bytes_done: done.min(total),
            bytes_total: total,
        });
    }
}
