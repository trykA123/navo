//! The compare engine: pairing, row checks, text and byte diffs, and safe copies.
//!
//! # What a screen must do with the engine
//!
//! A large text diff (`text::large::LargeDiff`) streams `DiffEvent`s on a bounded channel.
//! The order of events:
//!
//! - `Step`: how a section is handled (detailed, coarse or paged).
//! - `Provisional` and `Hunks`: early and final hunks, batch by batch.
//! - `Paged` with `seq`: a section drops its line hunks and keeps a count.
//! - `Progress`: a hint that may be dropped. `Stage::Waiting` means a section
//!   waits for memory that only an acknowledgement can free.
//! - `Restarted`: a file changed on disk and the diff starts over (at most 4 times).
//! - `Done`: the final result. `LargeDiff::outcome` keeps it if the event is lost.
//!
//! Rules for every screen:
//!
//! 1. Drain `events()` continuously. When the channel is full the driver
//!    blocks until the diff is cancelled or the receiver is dropped.
//! 2. After `Restarted`, drop everything received earlier: hunks, counts,
//!    steps and paged state. `Done(Err(..))` can follow partial output.
//! 3. Keep the highest `Paged.seq` you applied and pass it to
//!    `acknowledge_evictions`. Seq 0 (counts only) releases nothing. Sequences
//!    stay monotonic for a whole diff, across restarts. Evictions returned by
//!    `set_focus` are a second channel: call `apply`, then `acknowledge`. If the
//!    view never acknowledges, the diff fills its memory ledger and stays in
//!    `Stage::Waiting` until cancelled. Call `set_focus` only after you have
//!    drained the event channel through `Done`: a focus change while `Done` is
//!    queued but unread can evict hunks the view has not received.
//! 4. `Covered.hunks` holds placeholder blocks for coarse and paged parts
//!    (listed in `coarse`). Never offer them as copyable hunks unless
//!    `hunks_for` re-diffed them. Copying a coarse block rewrites line endings
//!    and whitespace-only lines.
//! 5. `hunks_for` reads synchronously, up to the 2 s watchdog plus a blocked
//!    read. Call it off the UI thread. `ChangedOnDisk` after `Done` means the
//!    screen must restart the diff.
//! 6. `take_changed` and `take_rewritten` reset their flag when read. Give the
//!    viewer, the diff and apply their own source instances.
//! 7. Dropping a `session::Session`, or calling `refresh()`, joins its
//!    workers. Opening a source and the pairing sort cannot be cancelled.
//!    Never drop a `Session` on the UI thread.
//! 8. A copy from a Git revision in worktree form fails with
//!    `SourceFilterFailed` when the filter writes anything to stderr. The
//!    failure comes at open, or at the end of a streamed read as an I/O error
//!    that the copy job maps to the same kind.
//! 9. A copy checks the bytes it copies (D101). `apply_hunk` and
//!    `apply_picked` revalidate both sources against their open baseline
//!    before and after writing, compare their sticky revisions, and look up
//!    the 128-bit BLAKE3 digests of every copyable hunk's source and target
//!    ranges in the side table that `Covered.digests` and `hunks_for` return
//!    (`HunkDigests`), keyed by hunk id. The table is built from the bytes the
//!    diff itself read, never a second read; apply re-reads and hashes both
//!    ranges. A mismatch is `ApplyError::Changed`. Open
//!    the apply sources when the diff starts so their baseline predates it. A
//!    retry stays refused because the open baseline is never refreshed and the
//!    hash re-check fails again. Coarse, paged and token-cap hunks have no
//!    entry and refuse with `ApplyError::NotCopyable`. A same-length edit that restores the modified
//!    time inside the window between the hash re-read and the copy is not
//!    caught. After `Changed`, discard both sources and the diff and start
//!    again. `out` is staging: write the real target only after `Ok`, from a
//!    temporary file that the caller discards on `Err`. This covers every
//!    caller of those functions and every compare save.
//! 10. `Whole` keeps the raw bytes of both sides (up to the whole-file cap)
//!     while it lives.

#![forbid(unsafe_code)]

pub mod anchor;
pub mod bytes;
pub mod content;
pub mod image;
pub mod model;
pub mod pair;
pub mod plan;
pub mod queue;
pub mod rules;
pub mod session;
pub mod text;
