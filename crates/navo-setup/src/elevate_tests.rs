// Tests for the elevated helper's pure contract: argument round trip, SID rules, descriptor check, exit codes.

use std::ffi::OsString;
use std::path::PathBuf;

use super::{
    AceView, HELPER_FLAG, HelperArgs, HelperExit, admin_descriptor_ok, admin_root_sddl,
    helper_command_line, launch_parameters, parse_helper_args, valid_user_sid,
};
use crate::placement::FileId;

const USER: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";
const SYSTEM: &str = "S-1-5-18";
const ADMINS: &str = "S-1-5-32-544";

/// Splits a command line like `CommandLineToArgvW` does for arguments after the program name.
fn windows_argv(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                in_arg = true;
                push_backslashes(&mut chars, &mut current);
            }
            '"' => {
                in_arg = true;
                quoted = !quoted;
            }
            ' ' | '\t' if !quoted => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            other => {
                in_arg = true;
                current.push(other);
            }
        }
    }
    if in_arg {
        args.push(current);
    }
    args
}

/// Copies a run of backslashes that starts at the current one: before a `"`, each pair is one
/// backslash and an odd one escapes the quote; elsewhere they are literal.
fn push_backslashes(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, current: &mut String) {
    let mut slashes = 1;
    while chars.next_if_eq(&'\\').is_some() {
        slashes += 1;
    }
    if chars.peek() != Some(&'"') {
        current.extend(std::iter::repeat_n('\\', slashes));
        return;
    }
    current.extend(std::iter::repeat_n('\\', slashes / 2));
    if slashes % 2 == 1 && chars.next().is_some() {
        current.push('"');
    }
}

/// Builds helper arguments for a root with a fixed SID and folder id.
fn sample(root: &str) -> HelperArgs {
    HelperArgs {
        root: PathBuf::from(root),
        user_sid: USER.to_owned(),
        parent: FileId {
            volume: 0x00ab_cdef_0123_4567,
            id: 0x0000_0000_0000_0001_0000_0000_0000_00ff,
        },
    }
}

/// Converts string items to the `OsString` slice `parse_helper_args` takes.
fn os(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

/// Round-trips a command line through Windows argv rules, dropping the flag.
fn round_trip(args: &HelperArgs) -> Result<HelperArgs, HelperExit> {
    let argv = windows_argv(&helper_command_line(args).expect("valid root"));
    assert_eq!(argv.first().map(String::as_str), Some(HELPER_FLAG));
    let rest: Vec<OsString> = argv[1..].iter().map(OsString::from).collect();
    parse_helper_args(&rest)
}

#[test]
fn command_line_has_the_exact_shape() {
    assert_eq!(
        helper_command_line(&sample(r"C:\Program Files\Navo App")),
        Some(format!(
            r#"--elevated-create --root "C:\Program Files\Navo App" --for {USER} --parent 00abcdef01234567:000000000000000100000000000000ff"#
        ))
    );
}

#[test]
fn command_line_refuses_a_root_the_helper_would_refuse() {
    assert_eq!(helper_command_line(&sample(r"C:\Navo\..\Windows")), None);
    assert_eq!(helper_command_line(&sample(r"C:\Navo\..")), None);
}

#[cfg(unix)]
#[test]
fn command_line_refuses_a_root_without_utf8_form() {
    use std::os::unix::ffi::OsStringExt;
    let mut args = sample(r"C:\Navo");
    args.root = PathBuf::from(OsString::from_vec(b"C:\\Na\xffvo".to_vec()));
    assert_eq!(helper_command_line(&args), None);
}

#[test]
fn round_trip_keeps_a_root_with_spaces_and_x86() {
    let args = sample(r"C:\Program Files (x86)\Navo App");
    assert_eq!(round_trip(&args), Ok(args));
}

#[test]
fn round_trip_strips_trailing_separators() {
    let parsed = round_trip(&sample(r"C:\Program Files\Navo App\\")).expect("parses");
    assert_eq!(parsed.root, PathBuf::from(r"C:\Program Files\Navo App"));
    assert_eq!(parsed, sample(r"C:\Program Files\Navo App"));
    let single = round_trip(&sample("C:\\Program Files (x86)\\Navo\\")).expect("parses");
    assert_eq!(single, sample(r"C:\Program Files (x86)\Navo"));
}

#[test]
fn parse_refuses_wrong_counts_and_order() {
    let good = [
        "--root",
        r"C:\Navo",
        "--for",
        USER,
        "--parent",
        "00abcdef01234567:000000000000000100000000000000ff",
    ];
    assert!(parse_helper_args(&os(&good)).is_ok());
    assert_eq!(
        parse_helper_args(&os(&good[..5])),
        Err(HelperExit::BadArguments)
    );
    let mut extra = good.to_vec();
    extra.push("--x");
    assert_eq!(
        parse_helper_args(&os(&extra)),
        Err(HelperExit::BadArguments)
    );
    let swapped = [good[2], good[3], good[0], good[1], good[4], good[5]];
    assert_eq!(
        parse_helper_args(&os(&swapped)),
        Err(HelperExit::BadArguments)
    );
    assert_eq!(parse_helper_args(&[]), Err(HelperExit::BadArguments));
}

#[test]
fn parse_refuses_bad_root_sid_and_id() {
    let with = |root: &str, sid: &str, id: &str| {
        parse_helper_args(&os(&["--root", root, "--for", sid, "--parent", id]))
    };
    let id = "00abcdef01234567:000000000000000100000000000000ff";
    for root in [
        r"\\server\share\Navo",
        r"C:\",
        "Navo",
        r"C:\Navo%x",
        r"C:\a:b",
    ] {
        assert_eq!(
            with(root, USER, id),
            Err(HelperExit::BadArguments),
            "{root}"
        );
    }
    assert_eq!(with(r"C:\Navo", SYSTEM, id), Err(HelperExit::BadArguments));
    for bad in [
        "00ABCDEF01234567:000000000000000100000000000000ff",
        "0abcdef01234567:000000000000000100000000000000ff",
        "00abcdef01234567:00000000000000100000000000000ff",
        "00abcdef01234567000000000000000100000000000000ff",
        "+0abcdef01234567:000000000000000100000000000000ff",
        "00abcdef01234567:+00000000000000100000000000000ff",
        "00abcdef0123456g:000000000000000100000000000000ff",
        "00abcdef01234567:000000000000000100000000000000ff:",
    ] {
        assert_eq!(
            with(r"C:\Navo", USER, bad),
            Err(HelperExit::BadArguments),
            "{bad}"
        );
    }
}

#[test]
fn user_sids_accept_only_domain_or_entra_users() {
    for good in [
        USER,
        "S-1-5-21-0-0-0-1000",
        "S-1-5-21-4294967295-1-2-4294967295",
        "S-1-12-1-1-2-3-4",
        "S-1-12-1-3263168403-1251543410-2391458196-3712425316",
        "S-1-5-21-1-2-3-500",
    ] {
        assert!(valid_user_sid(good), "{good}");
    }
    for bad in [
        "",
        SYSTEM,
        ADMINS,
        "S-1-5-21-1-2-3-999",
        "S-1-5-21-1-2-3-501",
        "S-1-5-21-1-2-3-502",
        "S-1-5-21-1-2-3-512",
        "S-1-5-21-1-2-3-544",
        "S-1-5-21-01-2-3-1000",
        "S-1-5-21-1-2-3-01000",
        "S-1-5-21-1-2-3",
        "S-1-5-21-1-2-3-1000-1",
        "S-1-5-21-4294967296-2-3-1000",
        "S-1-5-21-+1-2-3-1000",
        "S-1-5-21--2-3-1000",
        "s-1-5-21-1-2-3-1000",
        "S-1-5-21-1-2-3-1000 ",
        "S-1-12-1-1-2-3",
        "S-1-12-2-1-2-3-4",
    ] {
        assert!(!valid_user_sid(bad), "{bad}");
    }
}

#[test]
fn admin_root_sddl_is_the_iw25_descriptor() {
    assert_eq!(
        admin_root_sddl(USER),
        format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;{USER})")
    );
}

/// The IW25 ACE set for `USER`, in descriptor order.
fn iw25_aces() -> Vec<AceView> {
    let ace = |mask: u32, sid: &str| AceView {
        kind: 0,
        flags: 0x3,
        mask,
        sid: sid.to_owned(),
    };
    vec![
        ace(0x1F_01FF, SYSTEM),
        ace(0x1F_01FF, ADMINS),
        ace(0x13_01BF, USER),
    ]
}

#[test]
fn descriptor_ok_accepts_the_iw25_set_in_any_order() {
    let mut aces = iw25_aces();
    assert!(admin_descriptor_ok(ADMINS, true, &aces, USER));
    aces.reverse();
    assert!(admin_descriptor_ok(ADMINS, true, &aces, USER));
}

#[test]
fn descriptor_owned_by_the_user_is_refused() {
    assert!(!admin_descriptor_ok(USER, true, &iw25_aces(), USER));
    assert!(!admin_descriptor_ok(SYSTEM, true, &iw25_aces(), USER));
    assert!(!admin_descriptor_ok("", true, &iw25_aces(), USER));
}

#[test]
fn descriptor_ok_refuses_every_deviation() {
    assert!(!admin_descriptor_ok(ADMINS, false, &iw25_aces(), USER));
    let other = "S-1-5-21-1-2-3-1002";
    assert!(!admin_descriptor_ok(ADMINS, true, &iw25_aces(), other));
    assert!(!admin_descriptor_ok(ADMINS, true, &iw25_aces()[..2], USER));
    let mut extra = iw25_aces();
    extra.push(extra[2].clone());
    assert!(!admin_descriptor_ok(ADMINS, true, &extra, USER));
    let mut duplicate = iw25_aces();
    duplicate[1] = duplicate[0].clone();
    assert!(!admin_descriptor_ok(ADMINS, true, &duplicate, USER));
    type Edit = fn(&mut AceView);
    let edits: [Edit; 5] = [
        |a| a.kind = 1,
        |a| a.flags = 0x13,
        |a| a.flags = 0x1,
        |a| a.mask ^= 0x4_0000,
        |a| a.sid = "S-1-1-0".to_owned(),
    ];
    for (n, edit) in edits.iter().enumerate() {
        for index in 0..3 {
            let mut aces = iw25_aces();
            edit(&mut aces[index]);
            assert!(
                !admin_descriptor_ok(ADMINS, true, &aces, USER),
                "edit {n} ace {index}"
            );
        }
    }
}

#[test]
fn descriptor_ok_refuses_generic_all_on_the_user_ace() {
    let mut aces = iw25_aces();
    aces[2].mask |= 0x1000_0000;
    assert!(!admin_descriptor_ok(ADMINS, true, &aces, USER));
}

#[test]
fn descriptor_ok_refuses_administrators_or_system_as_the_user() {
    for user in [ADMINS, SYSTEM] {
        assert!(
            !admin_descriptor_ok(ADMINS, true, &iw25_aces(), user),
            "{user}"
        );
    }
}

#[test]
fn descriptor_ok_refuses_a_user_that_is_not_a_user_sid() {
    let ace = |mask: u32, sid: &str| AceView {
        kind: 0,
        flags: 0x3,
        mask,
        sid: sid.to_owned(),
    };
    let aces = [
        ace(0x1F_01FF, SYSTEM),
        ace(0x1F_01FF, ADMINS),
        ace(0x13_01BF, "S-1-1-0"),
    ];
    assert!(!admin_descriptor_ok(ADMINS, true, &aces, "S-1-1-0"));
}

#[test]
fn helper_exit_codes_round_trip() {
    let all = [
        (HelperExit::Created, 0),
        (HelperExit::NotElevated, 20),
        (HelperExit::BadArguments, 21),
        (HelperExit::Refused, 22),
        (HelperExit::Changed, 23),
        (HelperExit::Exists, 24),
        (HelperExit::CreateFailed, 25),
    ];
    for (exit, code) in all {
        assert_eq!(exit.code(), code);
        assert_eq!(HelperExit::from_code(code), Some(exit));
    }
    for code in [1, 19, 26, 255, u32::MAX] {
        assert_eq!(HelperExit::from_code(code), None, "{code}");
    }
}

#[test]
fn launch_parameters_are_the_command_line_nul_terminated() {
    let args = sample(r"C:\Program Files\Navo App");
    let line = helper_command_line(&args).expect("valid root");
    let wide = launch_parameters(&args).expect("valid arguments");
    assert_eq!(
        wide.split_last(),
        Some((&0, line.encode_utf16().collect::<Vec<_>>().as_slice()))
    );
}

#[test]
fn launch_parameters_refuse_before_any_prompt() {
    let mut bad_sid = sample(r"C:\Navo");
    bad_sid.user_sid = format!("{USER} --root C:\\Windows");
    let refused = [sample(r"C:\Navo\..\Windows"), sample("C:\\Na\0vo"), bad_sid];
    for args in refused {
        let error = launch_parameters(&args).expect_err("refused");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{args:?}");
    }
}
