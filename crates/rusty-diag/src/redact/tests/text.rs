use super::{Identity, Redactor, SALT_A, Salt, assert_absent, file, redactor, shape};

#[test]
fn user_name_is_replaced() {
    assert_eq!(
        redactor().text("Logged on as admin."),
        "Logged on as <user>."
    );
}

#[test]
fn computer_name_is_replaced() {
    assert_eq!(
        redactor().text("Host WORKPC-17 is up"),
        "Host <computer> is up"
    );
}

#[test]
fn domain_name_is_replaced() {
    assert_eq!(
        redactor().text("Joined domain CORP"),
        "Joined domain <domain>"
    );
}

#[test]
fn identity_names_are_replaced_case_insensitively() {
    assert_eq!(
        redactor().text("ADMIN on workpc-17 in Corp"),
        "<user> on <computer> in <domain>"
    );
}

#[test]
fn identity_names_are_replaced_inside_words() {
    assert_eq!(
        redactor().text("corp\\admin@workpc-17.corp.local"),
        "<domain>\\<user>@<computer>.<domain>.local"
    );
}

#[test]
fn windows_error_text_with_an_embedded_path_hides_every_name() {
    let out = redactor().text(r"Access is denied: 'C:\Users\admin\secret.txt'");
    assert_absent(&out, &["admin", "secret", "users"]);
}

#[test]
fn embedded_path_keeps_its_shape_and_the_quotes() {
    let out = redactor().text(r"Access is denied: 'C:\Users\admin\secret.txt'");
    assert_eq!(shape(&out), r"Access is denied: 'C:\<user>\<f>.txt'");
}

#[test]
fn drive_path_stops_at_whitespace_quotes_and_angle_brackets() {
    let out = redactor().text(r#"a D:\one\two.txt b "E:\three.txt" c <F:\four.txt> d|G:\five.txt"#);
    assert_eq!(
        shape(&out),
        r#"a D:\<d>\<f>.txt b "E:\<f>.txt" c <F:\<f>.txt> d|G:\<f>.txt"#
    );
}

#[test]
fn path_without_a_space_before_it_is_still_found() {
    let out = redactor().text(r"file=C:\Data\payroll.xlsx");
    assert_eq!(shape(&out), r"file=C:\<d>\<f>.xlsx");
}

#[test]
fn unc_path_inside_a_sentence() {
    let out = redactor().text(r"Copy to \\FileSrv01\Team\Plan.xlsx failed");
    assert_eq!(
        shape(&out),
        r"Copy to \\<server:1>\<share:1>\<f>.xlsx failed"
    );
}

#[test]
fn known_server_name_is_replaced_in_text() {
    let r = redactor();
    let _ = file(&r, r"\\FileSrv01\Team\Plan.xlsx");
    assert_eq!(
        r.text("Cannot reach filesrv01 right now"),
        "Cannot reach <server:1> right now"
    );
}

#[test]
fn text_shares_server_numbers_with_paths() {
    let r = redactor();
    let _ = file(&r, r"\\alpha\x\a.txt");
    let out = r.text(r"retry \\beta\y\b.txt then \\ALPHA\x\c.txt");
    assert_eq!(
        shape(&out),
        r"retry \\<server:2>\<share:2>\<f>.txt then \\<server:1>\<share:1>\<f>.txt"
    );
}

#[test]
fn long_path_prefix_in_text_is_dropped() {
    let out = redactor().text(r"open \\?\C:\Data\archive.bin failed");
    assert_eq!(shape(&out), r"open C:\<d>\<f>.bin failed");
}

#[test]
fn unix_profile_path_in_text_is_redacted() {
    let identity = Identity {
        user: Some("admin".to_owned()),
        profile: Some("/home/admin".to_owned()),
        ..Identity::default()
    };
    let r = Redactor::new(Salt::from_bytes(SALT_A), identity);
    let out = r.text("loaded /home/admin/notes/todo.md ok");
    assert_eq!(shape(&out), "loaded /<user>/<d>/<f>.md ok");
}

#[test]
fn several_paths_in_one_line_are_all_redacted() {
    let out = redactor().text(r"moved C:\inbox\alpha.txt to D:\archive\bravo.txt");
    assert_absent(&out, &["inbox", "alpha", "archive", "bravo"]);
}

#[test]
fn text_without_anything_to_redact_is_unchanged() {
    let line = "Copied 3 files (12.4 MB) in 120 ms; queue idle, 0 errors.";
    assert_eq!(redactor().text(line), line);
}

#[test]
fn empty_identity_fields_change_nothing() {
    let identity = Identity {
        user: Some(String::new()),
        computer: Some(String::new()),
        domain: Some(String::new()),
        profile: Some(String::new()),
    };
    let r = Redactor::new(Salt::from_bytes(SALT_A), identity);
    let line = "Copied 3 files in 120 ms";
    assert_eq!(r.text(line), line);
}

#[test]
fn empty_text_stays_empty() {
    assert_eq!(redactor().text(""), "");
}

#[test]
fn folders_with_spaces_inside_a_text_path_are_redacted() {
    let out = redactor().text(r"open C:\Data\My Docs\secret.txt and more");
    assert_eq!(shape(&out), r"open C:\<d>\<d>\<f>.txt and more");
}

#[test]
fn a_file_name_with_spaces_ends_at_its_extension() {
    let out = redactor().text(r"open C:\Data\secret report.txt failed");
    assert_eq!(shape(&out), r"open C:\<d>\<f>.txt failed");
}

#[test]
fn a_last_segment_without_extension_runs_to_the_hard_boundary() {
    let out = redactor().text("C:\\Data\\My Folder is locked\nnext line");
    assert_eq!(shape(&out), "C:\\<d>\\<f>\nnext line");
}

#[test]
fn a_second_path_in_the_same_run_starts_a_new_path() {
    let out = redactor().text(r"copy C:\a\x.txt to D:\b\y.txt done");
    assert_eq!(shape(&out), r"copy C:\<d>\<f>.txt to D:\<d>\<f>.txt done");
}
