#![forbid(unsafe_code)]

mod atomic_write;
mod db;

pub use atomic_write::{backup_path, write_atomic};
pub use db::{Db, DbError, OpenReport};
