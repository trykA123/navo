//! Maps Windows file attribute bits (`dwFileAttributes`) to Navo's `Attributes`.
//! A pure function, so the mapping is tested on every platform; the Windows listing,
//! stat and details paths all read their bits through it.

use crate::Attributes;

const READONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;
const OFFLINE: u32 = 0x1000;
const RECALL_ON_OPEN: u32 = 0x0004_0000;
const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
// Any one of these means the bytes live in the cloud and a read downloads them.
const CLOUD: u32 = OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS;

/// Turns the raw attribute bits of one file or folder into `Attributes`.
/// `link_to_folder` is left false: the caller decides it from the reparse tag.
pub(super) fn file_attributes(bits: u32) -> Attributes {
    Attributes {
        hidden: bits & HIDDEN != 0,
        system: bits & SYSTEM != 0,
        readonly: bits & READONLY != 0,
        cloud_placeholder: bits & CLOUD != 0,
        ..Attributes::default()
    }
}
