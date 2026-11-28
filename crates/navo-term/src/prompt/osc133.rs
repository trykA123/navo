//! OSC 133 (`vte` 0.15) stream parser for trusted prompt marks and command lifecycle events.
//! Preserves parser state across arbitrary read chunks and accepts only BEL (`\x07`) or
//! ST (`ESC \`) terminators.

use alacritty_terminal::vte::{Params, Parser, Perform};

/// Maximum command text length retained from `133;C` for display in `SyncWait::Running`.
pub(crate) const MAX_COMMAND_LEN: usize = 256;

/// One completed OSC 133 event extracted from PTY output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Osc133Event {
    /// `133;A`: carries the single `navo=<nonce>` parameter when valid, else `None`.
    PromptStart { navo_nonce: Option<String> },
    /// `133;B`: prompt end / user command input start.
    CommandStart,
    /// `133;C`: command execution began, with optional command text.
    CommandExecuted { command: Option<String> },
    /// `133;D`: command finished, with optional exit code.
    CommandFinished { exit_code: Option<i32> },
}

/// Stateful OSC 133 parser that survives arbitrary chunk splits, including between `ESC` and `\`.
pub(crate) struct Osc133Parser {
    parser: Parser,
    /// An OSC 133 event ended by `ESC` waiting for the matching `\` of ST (`ESC \`).
    pending_st: Option<Osc133Event>,
}

impl Default for Osc133Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Osc133Parser {
    pub(crate) fn new() -> Self {
        Self {
            parser: Parser::new(),
            pending_st: None,
        }
    }

    /// Feeds `bytes` until either the slice ends or a completed OSC 133 event terminates.
    /// Returns the consumed prefix slice and any completed OSC 133 event at its end.
    pub(crate) fn advance<'a>(&mut self, bytes: &'a [u8]) -> (&'a [u8], Option<Osc133Event>) {
        let mut performer = Performer {
            pending_st: &mut self.pending_st,
            completed: None,
        };
        let consumed = self.parser.advance_until_terminated(&mut performer, bytes);
        let slice = bytes.get(..consumed).unwrap_or(bytes);
        (slice, performer.completed)
    }
}

struct Performer<'a> {
    pending_st: &'a mut Option<Osc133Event>,
    completed: Option<Osc133Event>,
}

impl Perform for Performer<'_> {
    fn print(&mut self, _c: char) {
        *self.pending_st = None;
    }

    fn execute(&mut self, _byte: u8) {
        *self.pending_st = None;
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {
        *self.pending_st = None;
    }

    fn put(&mut self, _byte: u8) {
        *self.pending_st = None;
    }

    fn unhook(&mut self) {
        *self.pending_st = None;
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        *self.pending_st = None;
        let Some(event) = parse_osc_133(params) else {
            return;
        };
        if bell_terminated {
            self.completed = Some(event);
        } else {
            // `vte` calls `osc_dispatch` on the `ESC` of `ESC \`; wait for `\` to confirm ST.
            *self.pending_st = Some(event);
        }
    }

    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        *self.pending_st = None;
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if intermediates.is_empty() && !ignore && byte == b'\\' {
            if let Some(event) = self.pending_st.take() {
                self.completed = Some(event);
            }
        } else {
            *self.pending_st = None;
        }
    }

    fn terminated(&self) -> bool {
        self.completed.is_some()
    }
}

fn parse_osc_133(params: &[&[u8]]) -> Option<Osc133Event> {
    if params.first().copied() != Some(b"133".as_slice()) {
        return None;
    }
    let rest = params.get(2..).unwrap_or_default();
    match params.get(1).copied()? {
        b"A" => Some(Osc133Event::PromptStart {
            navo_nonce: parse_navo_nonce(rest),
        }),
        b"B" => Some(Osc133Event::CommandStart),
        b"C" => Some(Osc133Event::CommandExecuted {
            command: parse_command(rest),
        }),
        b"D" => Some(Osc133Event::CommandFinished {
            exit_code: parse_exit_code(rest),
        }),
        _ => None,
    }
}

/// Extracts the single `navo=<value>` parameter; duplicate or non-UTF-8 `navo=` params fail closed.
fn parse_navo_nonce(params: &[&[u8]]) -> Option<String> {
    let mut found: Option<String> = None;
    for &param in params {
        let Some(raw) = param.strip_prefix(b"navo=") else {
            continue;
        };
        if found.is_some() {
            return None;
        }
        let text = std::str::from_utf8(raw).ok()?;
        found = Some(text.to_owned());
    }
    found
}

fn parse_command(params: &[&[u8]]) -> Option<String> {
    let tagged = params.iter().find_map(|&param| {
        param
            .strip_prefix(b"cmdline=")
            .or_else(|| param.strip_prefix(b"command="))
    });
    let raw = tagged.or_else(|| match params {
        [single] if !single.contains(&b'=') => Some(*single),
        _ => None,
    })?;
    let text = std::str::from_utf8(raw).ok()?;
    sanitize_command(text)
}

fn parse_exit_code(params: &[&[u8]]) -> Option<i32> {
    let raw = params.first().copied()?;
    let text = std::str::from_utf8(raw).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<i32>()
        .ok()
        .or_else(|| text.parse::<u32>().ok().map(u32::cast_signed))
}

/// Strips control characters, trims whitespace and caps length on a UTF-8 char boundary.
pub(crate) fn sanitize_command(text: &str) -> Option<String> {
    let mut out = String::new();
    for ch in text.trim().chars() {
        if ch.is_control() {
            continue;
        }
        if out.len().saturating_add(ch.len_utf8()) > MAX_COMMAND_LEN {
            break;
        }
        out.push(ch);
    }
    (!out.is_empty()).then_some(out)
}
