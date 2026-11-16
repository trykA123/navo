//! What a listed entry is: a file, a folder or a link.
//!
//! Shared leaf type so `navo-fs` and the compare stack agree (NC111).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
    Link,
}
