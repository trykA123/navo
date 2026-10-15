#![forbid(unsafe_code)]

mod atomic_write;

pub use atomic_write::{backup_path, write_atomic};
