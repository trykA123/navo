use rusty_core::{Location, ProviderPath};

use super::super::Target;

pub(super) fn target(base: &Location, relative: &str, line: u64, archives: bool) -> Option<Target> {
    if archives {
        for (index, _) in relative.match_indices('\\') {
            let location = joined(base, &relative[..index])?;
            if rusty_fs::os_path(&location).is_ok_and(|path| path.is_file()) {
                return Some(Target::Content {
                    location,
                    line,
                    entry: Some(relative[index + 1..].replace('\\', "/")),
                });
            }
        }
    }
    Some(Target::Content {
        location: joined(base, relative)?,
        line,
        entry: None,
    })
}

fn joined(base: &Location, relative: &str) -> Option<Location> {
    let path = std::path::Path::new(relative);
    if path.is_absolute()
        || relative.as_bytes().get(1) == Some(&b':')
        || relative.starts_with("\\\\")
    {
        return Location::new(relative.replace('\\', std::path::MAIN_SEPARATOR_STR)).ok();
    }
    relative
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .try_fold(base.clone(), |folder, part| {
            folder.join(&ProviderPath::from_text(part)).ok()
        })
}
