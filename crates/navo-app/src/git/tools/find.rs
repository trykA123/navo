use std::path::{Path, PathBuf};

use navo_shell::Hive;

use super::ToolEnv;
use super::specs::{Spec, UNINSTALL_ROOTS};

const HIVES: [Hive; 2] = [Hive::LocalMachine, Hive::CurrentUser];

/// The first source that names an existing file wins; a stale source is skipped.
pub(super) fn first_existing(spec: &Spec, env: &impl ToolEnv) -> Option<PathBuf> {
    let from_path = spec.path.iter().filter_map(|name| env.on_path(name));
    let found: Vec<PathBuf> = if env.windows() {
        registry_candidates(spec, env)
            .into_iter()
            .chain(folder_candidates(spec, env))
            .chain(from_path)
            .collect()
    } else {
        from_path.collect()
    };
    found.into_iter().find(|path| env.is_file(path))
}

fn registry_candidates(spec: &Spec, env: &impl ToolEnv) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for key in spec.keys {
        found.extend(read(env, key.key, (key.value, key.folder), spec.exes));
    }
    for name in spec.uninstall {
        for root in UNINSTALL_ROOTS {
            let key = format!(r"{root}\{name}");
            found.extend(read(env, &key, ("InstallLocation", true), spec.exes));
            found.extend(read(env, &key, ("DisplayIcon", false), spec.exes));
        }
    }
    found
}

fn read(env: &impl ToolEnv, key: &str, value: (&str, bool), exes: &[&str]) -> Vec<PathBuf> {
    let (value, folder) = value;
    let mut found = Vec::new();
    for hive in HIVES {
        let Some(text) = env.registry(hive, key, value) else {
            continue;
        };
        if folder {
            let base = text.trim().trim_matches('"');
            found.extend(exes.iter().map(|exe| Path::new(base).join(exe)));
        } else {
            found.extend(exe_in(&text));
        }
    }
    found
}

fn folder_candidates(spec: &Spec, env: &impl ToolEnv) -> Vec<PathBuf> {
    spec.folders
        .iter()
        .filter_map(|(var, rest)| Some(Path::new(&env.var(var)?).join(rest)))
        .collect()
}

/// Pulls the program out of a registry command line or icon value.
fn exe_in(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    let program = match text.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => match text.to_ascii_lowercase().find(".exe") {
            Some(at) => &text[..at + 4],
            None => text.rsplit_once(',').map_or(text, |(path, _)| path),
        },
    };
    (!program.is_empty()).then(|| PathBuf::from(program))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::exe_in;

    #[test]
    fn a_registry_value_gives_its_program() {
        let want = Some(PathBuf::from(r"C:\a b\x.exe"));
        assert_eq!(exe_in(r#""C:\a b\x.exe" "%1""#), want);
        assert_eq!(exe_in(r"C:\a b\x.exe,0"), want);
        assert_eq!(exe_in(r"C:\a b\x.exe --flag"), want);
        assert_eq!(
            exe_in(r"C:\a b\x.cmd,2"),
            Some(PathBuf::from(r"C:\a b\x.cmd"))
        );
        assert_eq!(exe_in("  "), None);
    }
}
