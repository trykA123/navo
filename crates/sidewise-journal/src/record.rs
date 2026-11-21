//! The journal record codec (NC143): one record per log line, `<compact json>\t<16 hex>\n`,
//! where the hex is the first 8 bytes of the blake3 hash of the json.

use navo_core::FileStamp;
use serde::{Deserialize, Serialize};

/// The id of one job, `{created_ms:013}-{pid}-{seq}`; it is also the job's folder name.
/// A decoded id must have that shape, so it can never name a path outside the journal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JobId(String);

impl TryFrom<String> for JobId {
    type Error = String;

    /// Accepts `<13 to 20 digits>-<digits>-<digits>`; 20 digits is the widest `u64` time.
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = text.split('-').collect();
        let digits = |part: &str, min: usize| {
            (min..=20).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit())
        };
        match parts.as_slice() {
            [time, pid, seq] if digits(time, 13) && digits(pid, 1) && digits(seq, 1) => {
                Ok(Self(text))
            }
            _ => Err(format!("not a job id: {text:?}")),
        }
    }
}

impl From<JobId> for String {
    fn from(id: JobId) -> Self {
        id.0
    }
}

impl JobId {
    /// Builds the id from the creation time (ms since the Unix epoch), the process id and the
    /// per-process job counter.
    pub fn new(created_ms: u64, pid: u32, seq: u64) -> Self {
        Self(format!("{created_ms:013}-{pid}-{seq}"))
    }

    /// The id as text, as written in the log and used as the folder name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a job does, as the user would name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobKind {
    Copy,
    CopyMany,
    Undo,
    Sync,
}

/// What one item of a job does to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemMode {
    Overwrite,
    Create,
    Remove,
}

/// One line of a job log. The variants and their order in a log follow NC143 and NC144.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", deny_unknown_fields)]
pub enum Record {
    /// The first line of every job. `owner` is the stem of the owner lock, `<pid>-<start_ms>`.
    Begin {
        job: JobId,
        kind: JobKind,
        label: String,
        created_ms: u64,
        reverts: Option<JobId>,
        owner: String,
    },
    /// An item is about to be written; `target` is an absolute UTF-8 OS path.
    Intent {
        item: u32,
        target: String,
        mode: ItemMode,
        expect_size: Option<u64>,
        expect_mtime_ms: Option<i64>,
        restore_mtime_ms: Option<i64>,
    },
    /// The new bytes are staged and synced beside the target.
    Staged { item: u32, new: FileStamp },
    /// The old bytes are in a synced backup.
    Backed { item: u32, old: FileStamp },
    /// The target holds the new bytes.
    Done { item: u32 },
    /// The commit point: the job is complete.
    Commit,
    /// The job was rolled back; the log is closed.
    Aborted { reason: String },
}

/// The records of one log, in order, and what was wrong with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Log {
    /// Every good line before the first bad one.
    pub records: Vec<Record>,
    /// The last line had no `\n`; it was ignored.
    pub torn_tail: bool,
    /// A line that ends in `\n` failed its check; nothing after it was read.
    pub damaged: bool,
    /// The byte end of the last good, newline-terminated line (0 when there is none).
    /// Before a writer appends to a log with a torn tail, it cuts the file to this length
    /// (`set_len(good_len)`, then `sync_data`); else the new line joins the torn fragment
    /// and the job reads as damaged.
    pub good_len: usize,
}

/// Hex digits of the checksum: 8 bytes of blake3, lowercase.
const CHECK_LEN: usize = 16;

/// Encodes one record as a log line that ends in `\n`. The line is pure ASCII, so a crash
/// that cuts it at any byte still leaves valid UTF-8 for `decode`.
pub fn encode(record: &Record) -> String {
    let json = ascii_json(record);
    let check = checksum(&json);
    format!("{json}\t{check}\n")
}

/// Decodes the raw bytes of a whole log; it never fails, a bad log is reported in the flags
/// of `Log`. A last line with no `\n` is a torn tail and is ignored, whatever its bytes, even
/// when its checksum holds: a line counts only once its newline is written. A complete line
/// that is not UTF-8 or fails its check is damage, and reading stops there.
pub fn decode(bytes: &[u8]) -> Log {
    let mut log = Log::default();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(end) = rest.iter().position(|&byte| byte == b'\n') else {
            log.torn_tail = true;
            break;
        };
        let Some(record) = std::str::from_utf8(&rest[..end]).ok().and_then(decode_line) else {
            log.damaged = true;
            break;
        };
        log.records.push(record);
        log.good_len += end + 1;
        rest = &rest[end + 1..];
    }
    log
}

/// Decodes one line without its `\n`; `None` when its shape, checksum or json is wrong.
/// The json must be exactly what `encode` writes for the record it decodes to, so no
/// unknown field, nested ones in `FileStamp` included, is ever dropped in silence.
fn decode_line(line: &str) -> Option<Record> {
    let (json, check) = line.rsplit_once('\t')?;
    let well_formed = check.len() == CHECK_LEN
        && check
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if !well_formed || checksum(json) != check {
        return None;
    }
    let record: Record = serde_json::from_str(json).ok()?;
    (ascii_json(&record) == json).then_some(record)
}

/// The first 8 bytes of the blake3 hash of `json`, as 16 lowercase hex digits.
fn checksum(json: &str) -> String {
    let hash = blake3::hash(json.as_bytes());
    let mut hex = String::with_capacity(CHECK_LEN);
    for byte in &hash.as_bytes()[..CHECK_LEN / 2] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Serialises a record as compact json, with every non-ASCII char escaped as `\uXXXX`
/// (UTF-16 units, so a char past U+FFFF becomes a surrogate pair).
#[expect(
    clippy::expect_used,
    reason = "a Record has only string keys and plain fields, so serde_json cannot fail on it"
)]
fn ascii_json(record: &Record) -> String {
    let raw = serde_json::to_string(record).expect("a Record always serialises to json");
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii() {
            out.push(ch);
            continue;
        }
        let mut units = [0_u16; 2];
        for unit in ch.encode_utf16(&mut units) {
            out.push_str(&format!("\\u{unit:04x}"));
        }
    }
    out
}
