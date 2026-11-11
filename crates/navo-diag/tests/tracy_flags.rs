//! The Tracy build guard's environment rules (SC14), checked before any flag reaches `cc`.

include!("../tracy/flags.rs");

use std::collections::HashMap;
use std::path::Path;

/// The header a force-include has to name.
const HEADER: &str = r"D:\a\navo\crates\navo-diag\tracy\navo_tracy_user.hpp";
/// A different file, on disk, whose name only ends in the header's name.
const NEAR: &str = r"C:\x\my_navo_tracy_user.hpp";
/// The same near miss with forward slashes.
const NEAR_SLASH: &str = "/tmp/x/my_navo_tracy_user.hpp";
/// The sentence every refusal ends with.
const TAIL: &str = "use -FI<path> (MSVC) or -include <path> (gcc, clang) to crates/navo-diag/tracy/navo_tracy_user.hpp";
/// The target a tester build compiles the client for.
const TARGET: &str = "x86_64-pc-windows-msvc";
/// The one variable the guard reads flags from.
const FLAGS: &str = "TRACY_CLIENT_SYS_CXXFLAGS";

/// Runs the guard over a fake environment, on a file system that holds `HEADER` and `NEAR`.
fn check(env: &[(&str, &str)]) -> Result<(), String> {
    let vars: HashMap<String, String> = env
        .iter()
        .copied()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    let get = |name: &str| vars.get(name).cloned();
    let exists = |path: &Path| path == Path::new(HEADER) || path == Path::new(NEAR);
    check_env(TARGET, &get, &exists)
}

/// The flags of one force-include of `HEADER`, the shape a tester build is expected to use.
fn valid_flags() -> String {
    format!("-FI{HEADER}")
}

/// Refuses `env`, returning the message once it is known to end with the guard's advice. An
/// accepted environment fails here with the environment named, so no test has to panic.
fn refusal(env: &[(&str, &str)]) -> String {
    let error = check(env)
        .err()
        .unwrap_or_else(|| format!("the guard accepted {env:?}"));
    assert!(error.ends_with(TAIL), "{error}");
    error
}

/// Accepts a flags value held in `FLAGS`.
fn accepted(flags: &str) {
    let error = check(&[(FLAGS, flags)]).err().unwrap_or_default();
    assert!(error.is_empty(), "`{flags}` was refused: {error}");
}

/// Each compiler's spelling of one force-include of the header passes, with or without a
/// trim-path flag next to it.
#[test]
fn accepts_one_force_include_of_the_header() {
    accepted(&format!("-FI{HEADER}"));
    accepted(&format!("/FI{HEADER}"));
    accepted(&format!("-include {HEADER}"));
    accepted(&format!("-FI{HEADER} -d1trimfile:D:\\a\\navo\\navo\\"));
}

/// A build that sets no flag source at all cannot have the header force-included.
#[test]
fn refuses_a_missing_flags_variable() {
    let error = refusal(&[]);
    assert!(
        error.contains(FLAGS) && error.contains("is not set"),
        "{error}"
    );
}

/// A define naming the header is not an include: the client never sees the hook.
#[test]
fn refuses_a_define_of_the_header_name() {
    let flags = "-DX=navo_tracy_user.hpp";
    let error = refusal(&[(FLAGS, flags)]);
    assert!(error.contains(flags), "{error}");
}

/// A force-include of a file that is not on disk fails to build, so the guard says so first.
#[test]
fn refuses_a_force_include_of_a_missing_file() {
    let error = refusal(&[(FLAGS, r"-FIE:\absent\navo_tracy_user.hpp")]);
    assert!(
        error.contains(r"E:\absent\navo_tracy_user.hpp") && error.contains("does not exist"),
        "{error}"
    );
}

/// `-include` with nothing after it names no file.
#[test]
fn refuses_a_bare_include() {
    let error = refusal(&[(FLAGS, "-include")]);
    assert!(
        error.contains("-include") && error.contains("has no path"),
        "{error}"
    );
}

/// A force-include of another header leaves the hook out.
#[test]
fn refuses_a_path_ending_in_another_name() {
    let error = refusal(&[(FLAGS, r"-FID:\a\navo\other.hpp")]);
    assert!(
        error.contains(r"D:\a\navo\other.hpp") && error.contains("navo_tracy_user.hpp"),
        "{error}"
    );
}

/// A name that only ends in the header's name is another file, whichever separator the path
/// uses, and even when that file really is on disk.
#[test]
fn refuses_a_file_name_that_only_ends_in_the_header() {
    let error = refusal(&[(FLAGS, &format!("-FI{NEAR}"))]);
    assert!(error.contains(NEAR) && error.contains("is not"), "{error}");
    let error = refusal(&[(FLAGS, &format!("-include {NEAR_SLASH}"))]);
    assert!(
        error.contains(NEAR_SLASH) && error.contains("is not"),
        "{error}"
    );
}

/// A second include makes it unknowable which header the client got.
#[test]
fn refuses_two_force_includes() {
    let error = refusal(&[(FLAGS, &format!("-FI{HEADER} -FI{HEADER}"))]);
    assert!(error.contains("second"), "{error}");
}

/// An `-U` after the header would undo the define the hook needs.
#[test]
fn refuses_an_undefine_after_the_header() {
    let flags = &format!("-FI{HEADER} -UTRACY_HAS_CUSTOM_USER_INFO");
    let error = refusal(&[(FLAGS, flags)]);
    assert!(error.contains("-UTRACY_HAS_CUSTOM_USER_INFO"), "{error}");
}

/// Every other watched variable can add flags even when empty, so each one is refused by name.
#[test]
fn refuses_each_other_watched_variable() {
    let flags = valid_flags();
    for name in watched_vars(TARGET).iter().skip(1) {
        let error = refusal(&[(FLAGS, flags.as_str()), (name.as_str(), "")]);
        assert!(error.contains(name.as_str()), "{name}: {error}");
    }
}

/// `cc` reads the plain and the underscored spelling of the target, so the guard watches both.
#[test]
fn watched_vars_holds_both_target_spellings() {
    assert_eq!(
        watched_vars(TARGET),
        [
            FLAGS,
            "TRACY_CLIENT_SYS_CXXFLAGS_x86_64-pc-windows-msvc",
            "TRACY_CLIENT_SYS_CXXFLAGS_x86_64_pc_windows_msvc",
            "HOST_TRACY_CLIENT_SYS_CXXFLAGS",
            "TARGET_TRACY_CLIENT_SYS_CXXFLAGS",
            "TRACY_CLIENT_LIB",
            "TRACY_CLIENT_LIB_PATH",
            "TRACY_CLIENT_STATIC",
            "CC_SHELL_ESCAPED_FLAGS",
        ]
    );
}
