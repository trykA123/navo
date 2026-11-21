//! Content comparison with validated file stamps and cached hashes.

use std::time::{SystemTime, UNIX_EPOCH};

use blake3::Hasher;

use crate::model::{
    CancelToken, ContentClass, ContentKey, HashCache, ReadHint, Reason, Stamp, Status,
};

use super::hash::{block_for, update};
use super::read::{
    changed, check_kinds, check_object_ids, class, content_key, different, different_class,
    first_difference, outcome, read_block, stamp_matches, stopped, unavailable,
};
use super::stream::{Counts, Stream, probe_zero_sizes};
use super::{CheckOptions, ContentProgress, ContentResult, LARGE_CONTENT_THRESHOLD, Side};

type CachedHashes = (Option<[u8; 32]>, Option<[u8; 32]>);

/// A file modified less than this many ms before its check began may still be written
/// (git's "racy clean"), so its hash is not stored.
const RACY_MS: i64 = 10_000;

#[derive(Clone, Copy)]
pub(super) struct Sides<'a> {
    pub(super) left: Side<'a>,
    pub(super) right: Side<'a>,
}

#[derive(Clone, Copy)]
struct Cached<'a> {
    key: &'a ContentKey,
    hash: [u8; 32],
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
        started_ms: now_ms(),
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
    started_ms: Option<i64>,
    left_stamp: Stamp,
    right_stamp: Stamp,
    progress: &'b mut dyn FnMut(ContentProgress),
}

impl Checker<'_, '_> {
    /// Compares a pair using object IDs or validated metadata and content; returns read failures.
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
        let probed = match probe_zero_sizes(left, right) {
            Ok(probed) => probed,
            Err(result) => return result,
        };
        if let Some(result) = check_object_ids(left, right) {
            return result;
        }
        if left.entry.size != right.entry.size {
            return different(None, ContentClass::Unknown);
        }
        let left_key = content_key(left.source, left.entry);
        let right_key = content_key(right.source, right.entry);
        let keys = (&left_key, &right_key);
        let hashes = match self.cache_hashes(sides, keys, probed) {
            Ok(hashes) => hashes,
            Err(result) => return result,
        };
        self.compare_with_cache(sides, keys, hashes)
    }

    fn compare_with_cache(
        &mut self,
        sides: Sides<'_>,
        keys: (&ContentKey, &ContentKey),
        hashes: CachedHashes,
    ) -> ContentResult {
        match hashes {
            (Some(left_hash), Some(right_hash)) if left_hash == right_hash => {
                outcome(Status::Same, ContentClass::Unknown)
            }
            // NC208: a cached hash may only say Same; Different needs a lockstep read.
            (Some(_), Some(_)) => self.both(sides, keys),
            (Some(cached), None) => {
                let cached = Cached {
                    key: keys.1,
                    hash: cached,
                };
                let one = self.one_side(sides, sides.right, cached);
                one.unwrap_or_else(|| self.both(sides, keys))
            }
            (None, Some(cached)) => {
                let cached = Cached {
                    key: keys.0,
                    hash: cached,
                };
                let one = self.one_side(sides, sides.left, cached);
                one.unwrap_or_else(|| self.both(sides, keys))
            }
            (None, None) => self.both(sides, keys),
        }
    }

    fn cache_hashes(
        &mut self,
        sides: Sides<'_>,
        keys: (&ContentKey, &ContentKey),
        probed: (Option<Stamp>, Option<Stamp>),
    ) -> Result<CachedHashes, ContentResult> {
        let left_stamp = probed.0.map_or_else(
            || {
                sides
                    .left
                    .source
                    .stamp(&sides.left.entry.rel)
                    .map_err(unavailable)
            },
            Ok,
        )?;
        let right_stamp = probed.1.map_or_else(
            || {
                sides
                    .right
                    .source
                    .stamp(&sides.right.entry.rel)
                    .map_err(unavailable)
            },
            Ok,
        )?;
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

    fn one_side(
        &mut self,
        sides: Sides<'_>,
        read: Side<'_>,
        cached: Cached<'_>,
    ) -> Option<ContentResult> {
        let mut reader = match read.source.open(&read.entry.rel, self.read_hint) {
            Ok(reader) => reader,
            Err(reason) => return Some(unavailable(reason)),
        };
        let mut counts = Counts::new(read.entry.size);
        let mut hasher = Hasher::new();
        let mut saw_nul = false;
        let block = block_for(read.entry.size);
        let parallel = read.entry.size > LARGE_CONTENT_THRESHOLD;
        let mut buffer = vec![0u8; block];
        loop {
            if self.cancel.is_cancelled() {
                return Some(stopped());
            }
            let read_bytes = match read_block(&mut *reader, &mut buffer) {
                Ok(read) => read,
                Err(_) => return Some(unavailable(Reason::ReadError)),
            };
            if counts.add(read_bytes, buffer.len()) {
                return Some(changed());
            }
            if read_bytes == 0 {
                let digest = Digest {
                    hash: *hasher.finalize().as_bytes(),
                    saw_nul,
                };
                return self.finish_one(sides, cached, digest);
            }
            saw_nul |= buffer[..read_bytes].contains(&0);
            update(&mut hasher, &buffer[..read_bytes], parallel);
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
        let parallel = sides.left.entry.size > LARGE_CONTENT_THRESHOLD;
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
            update(&mut left_hasher, &stream.left_buffer[..step.left], parallel);
            update(
                &mut right_hasher,
                &stream.right_buffer[..step.right],
                parallel,
            );
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

    fn finish_one(
        &self,
        sides: Sides<'_>,
        cached: Cached<'_>,
        digest: Digest,
    ) -> Option<ContentResult> {
        if let Err(result) = self.unchanged(sides) {
            return Some(result);
        }
        if digest.hash != cached.hash {
            return None;
        }
        self.store(cached.key, digest.hash);
        Some(outcome(Status::Same, class(digest.saw_nul)))
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
        self.store(keys.0, *hashers.0.finalize().as_bytes());
        self.store(keys.1, *hashers.1.finalize().as_bytes());
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

    /// Stores `hash` only when the file was modified at least `RACY_MS` before the check
    /// began; a missing time or an unreadable clock stores nothing.
    fn store(&self, key: &ContentKey, hash: [u8; 32]) {
        if let (Some(modified), Some(started)) = (key.modified, self.started_ms)
            && modified <= started.saturating_sub(RACY_MS)
        {
            self.cache.put(key, hash);
        }
    }

    fn report(&mut self, done: u64, total: u64) {
        (self.progress)(ContentProgress {
            bytes_done: done.min(total),
            bytes_total: total,
        });
    }
}

/// The wall clock in ms since the epoch; `None` when the clock is before 1970 or too large.
fn now_ms() -> Option<i64> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}
