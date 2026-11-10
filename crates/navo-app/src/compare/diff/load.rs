//! Loads one file pair for the file diff and the Stream (D24, D26).
//!
//! The loader decodes both sides, diffs Whole text, and hashes the raw bytes for Viewed.

use std::io::{ErrorKind, Read};
use std::sync::Arc;

use blake3::Hasher;
use navo_compare::model::{CancelToken, ReadHint, Reason, SideEntry, SideSource};
use navo_compare::text::side::{TextSide, WHOLE_LIMIT};
use navo_compare::text::{Alignment, Options, Whole, diff_lines};
use navo_core::encoding::Encoding;
use navo_core::paging::TextLayout;
use navo_ui::compare_diff::PairKind;

const HASH_BYTES: usize = 32;
const ZERO_HASH: [u8; HASH_BYTES] = [0; HASH_BYTES];
const HASH_CHUNK: usize = 64 * 1024;
/// The extensions the F3 image viewer recognises, kept in step with `kinds/image.rs`.
const IMAGE_EXTENSIONS: [&str; 14] = [
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "ico", "tif", "tiff", "svg",
    "svgz",
];

/// One side of a pair to load: its source and its listed entry.
pub(crate) type Side = (Arc<dyn SideSource>, SideEntry);

/// Everything `load_pair` needs: both sides, the rule options and the fallback encoding.
pub(crate) struct LoadInput {
    pub left: Option<Side>,
    pub right: Option<Side>,
    pub options: Options,
    pub fallback: &'static Encoding,
}

/// A loaded pair. `alignment` is set only when both sides are Whole text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Loaded {
    pub kind: PairKind,
    pub left: Option<Whole>,
    pub right: Option<Whole>,
    pub alignment: Option<Alignment>,
    pub viewed: [u8; 64],
    /// False when a present side could not be hashed, so `viewed` would collide on zeros.
    pub viewable: bool,
}

/// What one side's decode produced. `Opaque` is an image side that is not decoded.
enum SideLoad {
    Missing,
    Whole(Whole),
    Large(u64),
    Binary,
    Opaque,
    Unavailable(&'static str),
}

/// Load one pair without blocking the caller: decode, diff and hash on this thread.
///
/// A cancelled token returns `Unavailable` with the stopped reason before any read.
#[must_use]
pub(crate) fn load_pair(input: &LoadInput, cancel: &CancelToken) -> Loaded {
    if cancel.is_cancelled() {
        return unavailable(Reason::Stopped);
    }
    if input.left.is_none() && input.right.is_none() {
        return unavailable(Reason::NotFound);
    }
    let missing = input.left.is_none() || input.right.is_none();
    let image = is_image(&input.left) || is_image(&input.right);
    let left = load_if(input.left.as_ref(), image, input.fallback);
    let right = load_if(input.right.as_ref(), image, input.fallback);
    let kind = pair_kind(missing, image, &left, &right);
    let alignment = alignment_of(&left, &right, &input.options);
    let (viewed, viewable) = viewed_of(input, &left, &right, cancel);
    Loaded {
        kind,
        left: whole_of(left),
        right: whole_of(right),
        alignment,
        viewed,
        viewable,
    }
}

/// Decode one side, or leave an image side opaque so its bytes are not read as text.
fn load_if(side: Option<&Side>, image: bool, fallback: &'static Encoding) -> SideLoad {
    let Some(side) = side else {
        return SideLoad::Missing;
    };
    if image {
        return SideLoad::Opaque;
    }
    load_side(side, fallback)
}

/// Decode one side through the engine's text path.
fn load_side((source, entry): &Side, fallback: &'static Encoding) -> SideLoad {
    match TextSide::decode(source.as_ref(), entry, fallback) {
        Ok(TextSide::Whole(whole)) => SideLoad::Whole(whole),
        Ok(TextSide::Large(large)) => SideLoad::Large(large.len),
        Err(Reason::Unsupported) => SideLoad::Binary,
        Err(reason) => SideLoad::Unavailable(reason.fluent_id().as_str()),
    }
}

/// Resolve the pair kind in the brief's order: missing, image, unavailable, binary, large, text.
fn pair_kind(missing: bool, image: bool, left: &SideLoad, right: &SideLoad) -> PairKind {
    if missing {
        return PairKind::OneSided {
            left: !matches!(left, SideLoad::Missing),
        };
    }
    if image {
        return PairKind::Image;
    }
    match (left, right) {
        (SideLoad::Unavailable(id), _) | (_, SideLoad::Unavailable(id)) => {
            PairKind::Unavailable(id)
        }
        (SideLoad::Binary, _) | (_, SideLoad::Binary) => PairKind::Binary,
        (SideLoad::Large(_), _) | (_, SideLoad::Large(_)) => PairKind::Large {
            left: size_of(left),
            right: size_of(right),
        },
        _ => PairKind::Text,
    }
}

/// A side's size for the Large summary; a side without bytes counts as zero.
fn size_of(side: &SideLoad) -> u64 {
    match side {
        SideLoad::Whole(whole) => u64::try_from(whole.bytes.len()).unwrap_or(u64::MAX),
        SideLoad::Large(len) => *len,
        SideLoad::Missing | SideLoad::Binary | SideLoad::Opaque | SideLoad::Unavailable(_) => 0,
    }
}

/// Diff the Whole sides, using an empty side for a missing text side.
fn alignment_of(left: &SideLoad, right: &SideLoad, options: &Options) -> Option<Alignment> {
    match (left, right) {
        (SideLoad::Whole(left), SideLoad::Whole(right)) => {
            Some(diff_lines(left.line_set(), right.line_set(), options))
        }
        (SideLoad::Whole(left), SideLoad::Missing) => {
            let empty = empty_whole();
            Some(diff_lines(left.line_set(), empty.line_set(), options))
        }
        (SideLoad::Missing, SideLoad::Whole(right)) => {
            let empty = empty_whole();
            Some(diff_lines(empty.line_set(), right.line_set(), options))
        }
        _ => None,
    }
}

/// Take a side's Whole text when it has one.
fn whole_of(side: SideLoad) -> Option<Whole> {
    match side {
        SideLoad::Whole(whole) => Some(whole),
        _ => None,
    }
}

/// An empty UTF-8 side with one start, used as the other half of a one-sided pair.
fn empty_whole() -> Whole {
    Whole {
        layout: TextLayout::default(),
        lines: Vec::new(),
        starts: vec![0],
        bytes: Vec::new(),
        lossless: true,
    }
}

/// True when a side's name carries one of the viewer's image extensions.
fn is_image(side: &Option<Side>) -> bool {
    let Some((_, entry)) = side else {
        return false;
    };
    let name = entry.name().to_text();
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        IMAGE_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}

/// The 64-byte Viewed key (left hash, right hash, zeros for a missing side) and whether every
/// present side was hashed.
fn viewed_of(
    input: &LoadInput,
    left: &SideLoad,
    right: &SideLoad,
    cancel: &CancelToken,
) -> ([u8; 64], bool) {
    let left = hash_side(&input.left, left, cancel);
    let right = hash_side(&input.right, right, cancel);
    let mut viewed = [0u8; 64];
    viewed[..HASH_BYTES].copy_from_slice(&left.unwrap_or(ZERO_HASH));
    viewed[HASH_BYTES..].copy_from_slice(&right.unwrap_or(ZERO_HASH));
    (viewed, left.is_some() && right.is_some())
}

/// Hash one side: Whole bytes are ready, other kinds stream; a missing side hashes to zeros.
/// `None` when a present side cannot be hashed.
fn hash_side(
    side: &Option<Side>,
    loaded: &SideLoad,
    cancel: &CancelToken,
) -> Option<[u8; HASH_BYTES]> {
    match loaded {
        SideLoad::Whole(whole) => Some(*blake3::hash(&whole.bytes).as_bytes()),
        SideLoad::Missing => Some(ZERO_HASH),
        SideLoad::Large(_) | SideLoad::Binary | SideLoad::Opaque | SideLoad::Unavailable(_) => {
            let (source, entry) = side.as_ref()?;
            stream_hash(source.as_ref(), entry, cancel)
        }
    }
}

/// Stream a side through BLAKE3; `None` on a cancel, a read error or a size over 64 MiB.
fn stream_hash(
    source: &dyn SideSource,
    entry: &SideEntry,
    cancel: &CancelToken,
) -> Option<[u8; HASH_BYTES]> {
    if entry.size > WHOLE_LIMIT {
        return None;
    }
    let mut reader = source.open(&entry.rel, ReadHint::Buffered).ok()?;
    let mut hasher = Hasher::new();
    let mut buffer = [0u8; HASH_CHUNK];
    let mut total = 0u64;
    loop {
        if cancel.is_cancelled() {
            return None;
        }
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                total = total.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
                if total > WHOLE_LIMIT {
                    return None;
                }
                hasher.update(&buffer[..count]);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    Some(*hasher.finalize().as_bytes())
}

/// A loaded pair that carries only an unavailable reason.
fn unavailable(reason: Reason) -> Loaded {
    Loaded {
        kind: PairKind::Unavailable(reason.fluent_id().as_str()),
        left: None,
        right: None,
        alignment: None,
        viewed: [0u8; 64],
        viewable: false,
    }
}
