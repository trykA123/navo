pub const NAME: &str = "Navo";
pub const SLUG: &str = "navo";
pub const LEGACY_NAME: &str = "Ferrite";
pub const LEGACY_SLUG: &str = "ferrite";

pub const TEMP_SUFFIX: &str = ".navo-part";
pub const LEGACY_TEMP_SUFFIX: &str = ".ferrite-part";
pub const RENAME_PREFIX: &str = ".navo-rename-";
pub const LEGACY_RENAME_PREFIX: &str = ".ferrite-rename-";

pub fn is_part_name(name: &str) -> bool {
    name.ends_with(TEMP_SUFFIX) || name.ends_with(LEGACY_TEMP_SUFFIX)
}

pub fn is_rename_temp(name: &str) -> bool {
    name.starts_with(RENAME_PREFIX) || name.starts_with(LEGACY_RENAME_PREFIX)
}

#[cfg(test)]
mod tests;
