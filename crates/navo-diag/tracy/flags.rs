// The Tracy build guard's environment rules (SC14). `include!`d by navo-diag's build script
// and by tests/tracy_flags.rs, so this header is line comments: `include!` refuses inner doc
// comments. It declares no `use` for the same reason: the include!ing crate owns them.

/// The variable `cc` reads the client's C++ flags from.
const FLAGS_VAR: &str = "TRACY_CLIENT_SYS_CXXFLAGS";
/// The header the guard insists on, by file name, taken off the path on both separators:
/// `Path` only knows the host's, and a tester build always names a Windows path.
const HEADER_FILE: &str = "navo_tracy_user.hpp";
/// The end of every refusal: the one flag shape the guard accepts.
const ADVICE: &str = "use -FI<path> (MSVC) or -include <path> (gcc, clang) to crates/navo-diag/tracy/navo_tracy_user.hpp";

/// Names every environment variable that can change the flags handed to the Tracy client's
/// C++ build, `cc`'s own order first and the bypasses after it.
fn watched_vars(target: &str) -> Vec<String> {
    let target_u = target.replace(['-', '.'], "_");
    vec![
        FLAGS_VAR.to_owned(),
        format!("{FLAGS_VAR}_{target}"),
        format!("{FLAGS_VAR}_{target_u}"),
        "HOST_TRACY_CLIENT_SYS_CXXFLAGS".to_owned(),
        "TARGET_TRACY_CLIENT_SYS_CXXFLAGS".to_owned(),
        "TRACY_CLIENT_LIB".to_owned(),
        "TRACY_CLIENT_LIB_PATH".to_owned(),
        "TRACY_CLIENT_STATIC".to_owned(),
        "CC_SHELL_ESCAPED_FLAGS".to_owned(),
    ]
}

/// Refuses any environment where the client's flags could come from anywhere but one
/// force-include of the guard header, naming the variable or token that broke the rule.
fn check_env(
    target: &str,
    get: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&std::path::Path) -> bool,
) -> Result<(), String> {
    let vars = watched_vars(target);
    for name in vars.iter().skip(1) {
        if get(name.as_str()).is_some() {
            return Err(format!("`{name}` is set; {ADVICE}"));
        }
    }
    let Some(flags) = get(FLAGS_VAR) else {
        return Err(format!("`{FLAGS_VAR}` is not set; {ADVICE}"));
    };
    let tokens: Vec<&str> = flags.split_ascii_whitespace().collect();
    let mut included: Option<&str> = None;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        index += 1;
        let path = if token == "-include" {
            let Some(next) = tokens.get(index) else {
                return Err(format!("`{token}` has no path; {ADVICE}"));
            };
            index += 1;
            *next
        } else if let Some(path) = token.strip_prefix("-FI").or(token.strip_prefix("/FI")) {
            path
        } else if token.starts_with("-d1trimfile:") || token.starts_with("/d1trimfile:") {
            continue;
        } else {
            return Err(format!("`{token}` is not a force-include; {ADVICE}"));
        };
        if included.is_some() {
            return Err(format!("`{token}` is a second include; {ADVICE}"));
        }
        if path.rsplit(['/', '\\']).next() != Some(HEADER_FILE) {
            return Err(format!("`{path}` is not `{HEADER_FILE}`; {ADVICE}"));
        }
        if !exists(std::path::Path::new(path)) {
            return Err(format!("`{path}` does not exist; {ADVICE}"));
        }
        included = Some(path);
    }
    if included.is_none() {
        return Err(format!("`{FLAGS_VAR}` has no force-include; {ADVICE}"));
    }
    Ok(())
}
