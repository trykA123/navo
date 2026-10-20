mod file;
mod photo;

use std::sync::Arc;

use rusty_core::rename::FieldRegistry;

pub use file::FileFields;
pub use photo::ExifFields;

#[must_use]
pub fn builtin() -> FieldRegistry {
    let mut registry = FieldRegistry::new();
    registry.register(Arc::new(FileFields));
    registry.register(Arc::new(ExifFields));
    registry
}

#[cfg(test)]
mod tests;
