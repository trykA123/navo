//! ConPTY's start-up cursor query. `portable-pty` creates every pseudoconsole with
//! `PSEUDOCONSOLE_INHERIT_CURSOR`, so ConPTY first asks where the cursor is (`ESC[6n`) and
//! prints nothing, nor lets the shell run on, until the host answers. A new session's cursor
//! sits at the origin, so the reader answers `ESC[1;1R` itself and drops the query; later
//! cursor queries reach the grid like any other output.

use std::borrow::Cow;
use std::sync::mpsc::SyncSender;

/// Device Status Report: "where is the cursor?"
const QUERY: &[u8] = b"\x1b[6n";
/// The answer for a fresh session: row 1, column 1.
const REPLY: &[u8] = b"\x1b[1;1R";
/// Output held while waiting for the query; ConPTY sends only a few mode sequences first.
const HELD_LIMIT: usize = 256;

/// Holds the first output until the start-up query was answered or cannot come any more.
pub(super) struct Handshake {
    held: Vec<u8>,
    /// Where the answer goes; `None` once the handshake is over (or never ran).
    reply: Option<SyncSender<Vec<u8>>>,
}

impl Handshake {
    /// Answers the start-up query through `reply`; `None` passes all output straight on.
    pub(super) fn new(reply: Option<SyncSender<Vec<u8>>>) -> Self {
        Self {
            held: Vec::new(),
            reply,
        }
    }

    /// What of `bytes` to pass on now. Holds output while it could still precede the query
    /// (only complete or partial CSI sequences so far); answers and strips the query.
    pub(super) fn feed<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        let Some(reply) = &self.reply else {
            return Cow::Borrowed(bytes);
        };
        self.held.extend_from_slice(bytes);
        if let Some(at) = find(&self.held, QUERY) {
            if reply.try_send(REPLY.to_vec()).is_err() {
                tracing::warn!("terminal could not answer the console's cursor query");
            }
            self.held.drain(at..at.saturating_add(QUERY.len()));
            return Cow::Owned(self.finish());
        }
        if self.held.len() > HELD_LIMIT || !only_csi(&self.held) {
            return Cow::Owned(self.finish());
        }
        Cow::Owned(Vec::new())
    }

    /// Ends the handshake and returns the held output.
    pub(super) fn finish(&mut self) -> Vec<u8> {
        self.reply = None;
        std::mem::take(&mut self.held)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Whether `bytes` are only CSI sequences, the last one possibly cut short.
fn only_csi(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    loop {
        match rest {
            [] | [0x1b] => return true,
            [0x1b, b'[', tail @ ..] => {
                // Parameter and intermediate bytes, then one final byte.
                let body = tail.iter().position(|byte| !(0x20..=0x3f).contains(byte));
                let Some(end) = body else { return true };
                match tail.get(end) {
                    Some(0x40..=0x7e) => rest = tail.get(end + 1..).unwrap_or_default(),
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};

    use super::{HELD_LIMIT, Handshake};

    fn answering() -> (Handshake, Receiver<Vec<u8>>) {
        let (reply, replies) = mpsc::sync_channel(4);
        (Handshake::new(Some(reply)), replies)
    }

    #[test]
    fn the_start_up_query_is_answered_once_and_dropped_from_the_output() {
        let (mut handshake, replies) = answering();
        assert!(handshake.feed(b"\x1b[?9001h\x1b[").is_empty());
        assert_eq!(&*handshake.feed(b"6nhi"), b"\x1b[?9001hhi");
        assert_eq!(replies.try_recv().unwrap(), b"\x1b[1;1R");
        assert_eq!(&*handshake.feed(b"\x1b[6n"), b"\x1b[6n");
        assert!(replies.try_recv().is_err());
    }

    #[test]
    fn text_before_any_query_ends_the_handshake_unanswered() {
        let (mut handshake, replies) = answering();
        assert_eq!(&*handshake.feed(b"\x1b[0mC:\\>"), b"\x1b[0mC:\\>");
        assert_eq!(&*handshake.feed(b"\x1b[6n"), b"\x1b[6n");
        assert!(replies.try_recv().is_err());
    }

    #[test]
    fn held_output_is_released_past_the_limit_or_at_the_end() {
        let (mut handshake, _replies) = answering();
        let modes = b"\x1b[?25l".repeat(HELD_LIMIT / 6);
        assert!(handshake.feed(&modes).is_empty());
        assert_eq!(handshake.feed(b"\x1b[?25h").len(), modes.len() + 6);
        let (mut cut, _replies) = answering();
        assert!(cut.feed(b"\x1b[?25").is_empty());
        assert_eq!(cut.finish(), b"\x1b[?25");
    }

    #[test]
    fn without_a_reply_channel_output_passes_straight_through() {
        let mut handshake = Handshake::new(None);
        assert_eq!(&*handshake.feed(b"\x1b[6n"), b"\x1b[6n");
    }
}
