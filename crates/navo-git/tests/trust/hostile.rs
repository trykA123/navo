use super::trust_support::{GitDir, git_fixture, neutralised, pairs};

#[test]
fn section_and_key_names_match_in_any_case() {
    let guard = GitDir::new("[CoRe]\n\tSSHCOMMAND = x\n[FILTER \"f\"]\n\tCLEAN = x\n").guard();
    assert_eq!(guard.keys(), ["core.sshcommand", "filter.f.clean"]);
    assert!(neutralised(&guard, "f"));
}

#[test]
fn an_implicit_value_still_guards() {
    let guard = GitDir::new("[core]\n\tfsmonitor\n").guard();
    assert_eq!(guard.keys(), ["core.fsmonitor"]);
}

#[test]
fn a_quoted_filter_name_keeps_its_case() {
    let guard = GitDir::new("[filter \"EvIl\"]\n\tclean = x\n").guard();
    assert!(neutralised(&guard, "EvIl"));
}

#[test]
fn a_legacy_filter_name_is_lowercased_like_git() {
    let guard = GitDir::new("[filter.EvIl]\n\tclean = x\n").guard();
    assert!(neutralised(&guard, "evil"));
    assert!(guard.is_guarded());
}

#[test]
fn filter_names_with_dots_spaces_and_escapes_survive() {
    let config = "[filter \"a.b.c\"]\n\tclean = x\n[filter \"s p\"]\n\tsmudge = x\n\
                  [filter \"q\\\"x\\\\y\"]\n\tprocess = x\n[filter \"\"]\n\tclean = x\n";
    let guard = GitDir::new(config).guard();
    for name in ["a.b.c", "s p", "q\"x\\y", ""] {
        assert!(neutralised(&guard, name), "{name:?}: {:?}", pairs(&guard));
    }
    assert!(!guard.is_blocked());
}

#[test]
fn a_filter_name_that_cannot_be_neutralised_blocks() {
    for name in ["a=b", "=", "x="] {
        let guard = GitDir::new(&format!("[filter \"{name}\"]\n\tclean = x\n")).guard();
        assert!(guard.is_blocked(), "{name}");
    }
}

#[test]
fn include_keys_match_in_any_case() {
    let repo = GitDir::new("[INCLUDE]\n\tPATH = a.inc\n[IncludeIf \"gitdir:x\"]\n\tPath = b.inc\n");
    repo.write("repo/.git/a.inc", b"[gpg]\n\tprogram = x\n");
    repo.write("repo/.git/b.inc", b"[diff]\n\texternal = x\n");
    assert_eq!(repo.guard().keys(), ["diff.external", "gpg.program"]);
}

#[test]
fn a_header_and_value_on_one_line_count() {
    let guard = GitDir::new("[filter \"x\"] clean = y\n").guard();
    assert!(neutralised(&guard, "x"));
}

#[test]
fn control_characters_never_reach_the_key_names() {
    let guard = GitDir::new("[credential \"https://a\tb\"]\n\thelper = x\n").guard();
    assert!(guard.is_guarded());
    assert!(
        guard
            .keys()
            .iter()
            .all(|key| !key.chars().any(char::is_control))
    );
}

const CORPUS: &[&[u8]] = &[
    b"[filter \"Evil\"]\n\tclean = x\n",
    b"[FILTER \"e\"]\n\tCLEAN = x\n",
    b"[filter.Legacy]\n\tclean = x\n",
    b"[filter.a.b]\n\tclean = x\n",
    b"[filter \"a.b.c\"]\n\tsmudge = x\n",
    b"[filter \"q\\\"x\\\\y\"]\n\tprocess = x\n",
    b"[filter \"\\e\\s\"]\n\tclean = x\n",
    b"[filter\t\"tab\"]\n\tclean = x\n",
    b"[filter \"x\"] clean = y\n",
    b"[filter \"x\"]\n\tclean\n",
    b"\xEF\xBB\xBF[filter \"bom\"]\n\tclean = x\n",
    b"[filter \"crlf\"]\r\n\tclean = x\r\n",
    b"[filter \"c\"] ; note\n\tclean = x # note\n",
    b"[core]\n\tx = a\\\n[filter \"joined\"]\n\tclean = y\n",
    b"[core]\n\tx = \"a ; [filter \\\"q\\\"]\"\n[filter \"after\"]\nclean=1\n",
    b"[filter \"a=b\"]\n\tclean = x\n",
    b"[filter \"\"]\n\tclean = x\n",
    b"[ filter \"sp\"]\n\tclean = x\n",
    b"[filter \"x\" ]\n\tclean = x\n",
    b"[filter \"x\"]\n\tclean = x\n[remote \"o.r\"]\n\tuploadPack = x\n",
    b"[core]\n\tSshCommand = x\n\t[filter \"indented\"]\n\tclean = y\n",
    b"[filter.a \"b\"]\n\tclean = x\n",
    b"[Filter.A.B]\n\tclean = x\n",
    b"[filter.Up \"Low\"]\n\tclean = x\n",
    b"[remote.o \"r\"]\n\tuploadpack = x\n",
    b"[hook \"x\"]\n\tevent = reference-transaction\n\tcommand = x\n",
    b"[gc]\n\trecentObjectsHook = x\n",
];

#[test]
fn every_filter_and_guard_key_git_sees_is_covered() {
    let Some(fixture) = git_fixture() else {
        return;
    };
    let mut compared = 0;
    for (index, config) in CORPUS.iter().enumerate() {
        let repo = GitDir::with_bytes(config);
        let path = repo.git_dir.join("config");
        let output = fixture.git(&["config", "--file", path.to_str().unwrap(), "--list", "-z"]);
        if !output.status.success() {
            continue;
        }
        let guard = repo.guard();
        if guard.is_blocked() {
            continue;
        }
        let names = pairs(&guard);
        compared += 1;
        for key in git_keys(&output.stdout) {
            if let Some(name) = filter_name(&key) {
                let pair = format!("filter.{name}.clean=");
                assert!(
                    names.contains(&pair),
                    "corpus {index}: {key:?} not in {names:?}"
                );
            }
            if guarded_by_git(&key) {
                assert!(guard.is_guarded(), "corpus {index}: {key:?} did not guard");
            }
        }
    }
    assert!(
        compared >= CORPUS.len() - 6,
        "only {compared} configs compared"
    );
}

fn git_keys(stdout: &[u8]) -> Vec<String> {
    stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let key = record
                .split(|byte| *byte == b'\n')
                .next()
                .unwrap_or_default();
            String::from_utf8_lossy(key).into_owned()
        })
        .collect()
}

fn filter_name(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("filter.")?;
    let dot = rest.rfind('.')?;
    Some(&rest[..dot])
}

fn guarded_by_git(key: &str) -> bool {
    let section = key.split('.').next().unwrap_or_default();
    let name = key.rsplit('.').next().unwrap_or_default();
    matches!(
        (section, name),
        ("filter", _)
            | (
                "core",
                "sshcommand" | "fsmonitor" | "hookspath" | "gitproxy" | "askpass"
            )
            | ("remote", "uploadpack" | "receivepack")
            | ("hook", _)
            | ("gc", "recentobjectshook")
    )
}
