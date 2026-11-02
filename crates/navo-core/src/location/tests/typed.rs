use super::*;

const WINDOWS_HOME: &[(&str, &str)] = &[("USERPROFILE", r"C:\Users\admin")];

fn typed_with(input: &str, current: &str, vars: &[(&str, &str)]) -> Result<Location, String> {
    let env = |name: &str| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    };
    typed_location(input, &loc(current), &env)
}

fn typed(input: &str, current: &str) -> Result<Location, String> {
    typed_with(input, current, WINDOWS_HOME)
}

fn at_admin(input: &str) -> Result<Location, String> {
    typed(input, r"C:\Users\admin")
}

#[test]
fn an_absolute_drive_path_stays() {
    assert_eq!(at_admin(r"D:\x"), Ok(loc(r"D:\x")));
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    assert_eq!(at_admin("  \tD:\\x \n"), Ok(loc(r"D:\x")));
}

#[test]
fn copy_as_path_quotes_are_trimmed() {
    assert_eq!(
        at_admin(r#" "D:\Program Files\x" "#),
        Ok(loc(r"D:\Program Files\x"))
    );
}

#[test]
fn empty_input_is_an_error() {
    assert!(at_admin("").is_err());
}

#[test]
fn blank_or_quoted_empty_input_is_an_error() {
    assert!(at_admin("   ").is_err());
    assert!(at_admin(r#" "" "#).is_err());
}

#[test]
fn forward_slashes_become_backslashes_on_a_drive() {
    assert_eq!(at_admin("d:/x/y"), Ok(loc(r"d:\x\y")));
}

#[test]
fn a_bare_drive_means_its_root() {
    assert_eq!(at_admin("D:"), Ok(loc(r"D:\")));
}

#[test]
fn a_drive_root_keeps_its_separator() {
    assert_eq!(at_admin(r"D:\"), Ok(loc(r"D:\")));
}

#[test]
fn a_unc_path_stays() {
    assert_eq!(at_admin(r"\\nas\share\x"), Ok(loc(r"\\nas\share\x")));
}

#[test]
fn a_forward_slash_unc_path_becomes_backslashes() {
    assert_eq!(at_admin("//nas/share/x"), Ok(loc(r"\\nas\share\x")));
}

#[test]
fn a_variable_is_expanded_from_the_environment() {
    assert_eq!(
        at_admin(r"%USERPROFILE%\docs"),
        Ok(loc(r"C:\Users\admin\docs"))
    );
}

#[test]
fn a_variable_is_looked_up_with_its_case_as_given() {
    let vars = [("Data", r"E:\data")];
    assert_eq!(
        typed_with(r"%Data%\x", r"C:\Users\admin", &vars),
        Ok(loc(r"E:\data\x"))
    );
}

#[test]
fn an_unknown_variable_is_an_error_naming_it() {
    let error = at_admin(r"%NOPE%\x").unwrap_err();
    assert!(error.contains("NOPE"), "{error}");
}

#[test]
fn a_tilde_alone_is_the_user_profile() {
    assert_eq!(typed("~", r"D:\work"), Ok(loc(r"C:\Users\admin")));
}

#[test]
fn a_tilde_prefix_expands_from_the_user_profile() {
    assert_eq!(
        typed(r"~\docs", r"D:\work"),
        Ok(loc(r"C:\Users\admin\docs"))
    );
    assert_eq!(typed("~/docs", r"D:\work"), Ok(loc(r"C:\Users\admin\docs")));
}

#[test]
fn a_tilde_falls_back_to_home() {
    let vars = [("HOME", "/home/admin")];
    assert_eq!(
        typed_with("~/docs", "/tmp", &vars),
        Ok(loc("/home/admin/docs"))
    );
}

#[test]
fn a_relative_name_resolves_against_the_current_folder() {
    assert_eq!(at_admin("sub"), Ok(loc(r"C:\Users\admin\sub")));
}

#[test]
fn a_relative_path_resolves_against_the_current_folder() {
    assert_eq!(
        at_admin(r"sub\deeper"),
        Ok(loc(r"C:\Users\admin\sub\deeper"))
    );
    assert_eq!(
        at_admin("sub/deeper"),
        Ok(loc(r"C:\Users\admin\sub\deeper"))
    );
}

#[test]
fn dot_segments_are_dropped() {
    assert_eq!(at_admin(r".\sub"), Ok(loc(r"C:\Users\admin\sub")));
    assert_eq!(
        at_admin(r"sub\.\deeper"),
        Ok(loc(r"C:\Users\admin\sub\deeper"))
    );
}

#[test]
fn dot_dot_goes_up_one_folder() {
    assert_eq!(at_admin(".."), Ok(loc(r"C:\Users")));
    assert_eq!(at_admin(r"..\other"), Ok(loc(r"C:\Users\other")));
}

#[test]
fn dot_dot_is_resolved_inside_an_absolute_path() {
    assert_eq!(at_admin(r"D:\a\.\b\..\c"), Ok(loc(r"D:\a\c")));
}

#[test]
fn dot_dot_never_goes_above_a_drive_root() {
    assert_eq!(typed("..", r"C:\"), Ok(loc(r"C:\")));
    assert_eq!(at_admin(r"..\..\..\.."), Ok(loc(r"C:\")));
}

#[test]
fn dot_dot_inside_a_unc_share_goes_up() {
    assert_eq!(typed("..", r"\\nas\share\x\y"), Ok(loc(r"\\nas\share\x")));
}

#[test]
fn a_trailing_separator_is_dropped() {
    assert_eq!(at_admin(r"sub\"), Ok(loc(r"C:\Users\admin\sub")));
    assert_eq!(at_admin(r"D:\x\"), Ok(loc(r"D:\x")));
}

#[test]
fn a_unix_relative_path_resolves_against_the_current_folder() {
    assert_eq!(
        typed("docs/x", "/home/admin"),
        Ok(loc("/home/admin/docs/x"))
    );
}

#[test]
fn a_unix_absolute_path_stays() {
    assert_eq!(typed("/etc", "/home/admin"), Ok(loc("/etc")));
}

#[test]
fn unix_dot_dot_stops_at_the_slash_root() {
    assert_eq!(typed("..", "/home/admin"), Ok(loc("/home")));
    assert_eq!(typed("../../..", "/home/admin"), Ok(loc("/")));
}

#[test]
fn a_unix_absolute_path_on_a_windows_folder_is_an_error() {
    assert!(at_admin("/x").is_err());
}

#[test]
fn a_relative_path_on_another_provider_joins_that_provider() {
    assert_eq!(typed("b", "fake://home/a"), Ok(loc("fake://home/a/b")));
}

#[test]
fn a_rooted_path_starts_at_the_current_drive() {
    assert_eq!(at_admin(r"\foo"), Ok(loc(r"C:\foo")));
    assert_eq!(
        typed(r"\x\y", r"\\nas\media\films"),
        Ok(loc(r"\\nas\media\x\y"))
    );
}

#[test]
fn percent_signs_in_folder_names_stay_literal() {
    assert_eq!(
        at_admin(r"C:\50% off\100% real"),
        Ok(loc(r"C:\50% off\100% real"))
    );
    assert_eq!(at_admin("100%"), Ok(loc(r"C:\Users\admin\100%")));
}
