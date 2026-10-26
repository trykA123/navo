pub const NAME: &str = "Navo";
pub const SLUG: &str = "navo";
pub const LEGACY_NAME: &str = "Ferrite";
pub const LEGACY_SLUG: &str = "ferrite";

pub const TEMP_SUFFIX: &str = ".navo-part";
pub const LEGACY_TEMP_SUFFIX: &str = ".ferrite-part";
pub const RENAME_PREFIX: &str = ".navo-rename-";
pub const LEGACY_RENAME_PREFIX: &str = ".ferrite-rename-";

pub const RETAINED_PREFIX: &str = ".navo-retained-";
pub const LEGACY_RETAINED_PREFIX: &str = ".ferrite-retained-";
pub const HELD_PREFIX: &str = ".navo-held-";
pub const LEGACY_HELD_PREFIX: &str = ".ferrite-held-";
pub const MUTATIONS_PREFIX: &str = ".navo-mutations-";
pub const LEGACY_MUTATIONS_PREFIX: &str = ".ferrite-mutations-";

pub fn is_part_name(name: &str) -> bool {
    name.ends_with(TEMP_SUFFIX) || name.ends_with(LEGACY_TEMP_SUFFIX)
}

pub fn is_rename_temp(name: &str) -> bool {
    name.starts_with(RENAME_PREFIX) || name.starts_with(LEGACY_RENAME_PREFIX)
}

pub fn is_retained_container(name: &str) -> bool {
    starts_with_any(
        name,
        &[
            RETAINED_PREFIX,
            LEGACY_RETAINED_PREFIX,
            HELD_PREFIX,
            LEGACY_HELD_PREFIX,
        ],
    )
}

pub fn is_reserved_name(name: &str) -> bool {
    is_retained_container(name)
        || starts_with_any(name, &[MUTATIONS_PREFIX, LEGACY_MUTATIONS_PREFIX])
}

fn starts_with_any(name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        name.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}

#[cfg(test)]
mod tests;
