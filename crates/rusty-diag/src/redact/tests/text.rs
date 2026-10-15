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

fn user(name: &str) -> Redactor {
    let identity = Identity {
        user: Some(name.to_owned()),
        ..Identity::default()
    };
    Redactor::new(Salt::from_bytes(SALT_A), identity)
}

#[test]
fn a_dotted_number_before_the_extension_word_does_not_end_the_file_name() {
    let out = redactor().text(r"failed D:\Clients\Report 2024.03 Acme Merger.pdf");
    assert_eq!(shape(&out), r"failed D:\<d>\<f>.pdf");
}

#[test]
fn a_version_word_before_the_extension_word_does_not_end_the_file_name() {
    let out = redactor().text(r"C:\x\v1.2 final draft for Bob.docx");
    assert_eq!(shape(&out), r"C:\<d>\<f>.docx");
}

#[test]
fn a_dotted_user_name_as_a_drive_file_leaves_no_part_of_the_name() {
    let out = user("john.smith").text(r"open D:\backup\john.smith");
    assert_absent(&out, &["smith"]);
}

#[test]
fn a_dotted_user_name_as_a_unc_file_leaves_no_part_of_the_name() {
    let out = user("john.smith").text(r"\\fs01\home$\john.smith");
    assert_absent(&out, &["smith"]);
}

#[test]
fn forward_slash_unc_path_in_text_hides_server_share_and_file() {
    let out = redactor().text("open //fileserver01/finance/Payroll Q3.xlsx");
    assert_absent(&out, &["fileserver01", "finance", "payroll", "q3"]);
}

#[test]
fn a_url_is_not_a_path() {
    let line = "see https://example.com/a for help";
    assert_eq!(redactor().text(line), line);
}

#[test]
fn single_quoted_fragment_becomes_its_length() {
    assert_eq!(
        redactor().text("cannot rename 'Secret Plan.docx'"),
        "cannot rename '<text:16 chars>'"
    );
}

#[test]
fn double_quoted_fragment_becomes_its_length() {
    assert_eq!(
        redactor().text(r#"cannot rename "Secret Plan.docx""#),
        r#"cannot rename "<text:16 chars>""#
    );
}

#[test]
fn curly_quoted_fragment_becomes_its_length() {
    assert_eq!(
        redactor().text("cannot rename \u{201c}\u{e9}t\u{e9} Plan\u{201d}"),
        "cannot rename \u{201c}<text:8 chars>\u{201d}"
    );
}

#[test]
fn a_share_name_seen_in_a_path_is_replaced_in_later_text() {
    let out = redactor().text(r"open \\fs01\finance$\a.txt then share finance$ was slow");
    assert_eq!(
        shape(&out),
        r"open \\<server:1>\<share:1>\<f>.txt then share <share:1> was slow"
    );
}

#[test]
fn unix_path_outside_the_profile_after_a_space_is_redacted() {
    let out = redactor().text("open /mnt/data/Medical Records/scan.pdf failed");
    assert_absent(&out, &["medical", "records", "scan"]);
}

#[test]
fn unix_path_outside_the_profile_at_start_after_equals_or_quote_is_redacted() {
    let r = redactor();
    for line in [
        "/mnt/data/scan.pdf failed",
        "file=/mnt/data/scan.pdf",
        "open '/mnt/data/scan.pdf'",
    ] {
        assert_absent(&r.text(line), &["mnt", "data", "scan"]);
    }
}

#[test]
fn slashes_inside_words_and_numbers_are_not_paths() {
    let line = "ratio a/b is 1/2 and/or more";
    assert_eq!(redactor().text(line), line);
}

#[test]
fn a_quoted_fragment_holding_a_path_is_hidden_whole() {
    let out = redactor().text(r"cannot copy 'Secret Plan C:\x\a.txt' now");
    assert_absent(&out, &["secret", "plan"]);
}

#[test]
fn a_double_quoted_fragment_holding_two_names_and_a_path_is_hidden_whole() {
    let out = redactor().text(r#"cannot copy "Secret Plan.docx to D:\x\y.docx""#);
    assert_absent(&out, &["secret", "plan"]);
}

#[test]
fn an_apostrophe_inside_a_file_name_does_not_end_the_path() {
    let out = redactor().text(r"failed D:\Clients\O'Brien Merger.pdf");
    assert_eq!(shape(&out), r"failed D:\<d>\<f>.pdf");
}

#[test]
fn a_possessive_apostrophe_inside_a_file_name_does_not_end_the_path() {
    let out = redactor().text(r"failed D:\Clients\Bob's Plan.pdf");
    assert_absent(&out, &["bob", "plan"]);
}

#[test]
fn only_real_token_shapes_count_as_already_redacted() {
    let out = redactor().text("error '<payroll q3>' and '<acme merger>'");
    assert_absent(&out, &["payroll", "acme"]);
}

#[test]
fn a_drive_path_right_after_a_word_is_still_found() {
    let out = redactor().text(r"file1C:\Secret\a.txt");
    assert_absent(&out, &["secret"]);
}

#[test]
fn an_unclosed_quote_after_a_space_hides_the_rest() {
    let out = redactor().text("rename 'Secret Plan.docx");
    assert_eq!(out, "rename '<text:16 chars>");
}

#[test]
fn a_double_slash_comment_is_not_a_unc_path() {
    let line = "see // comment here";
    assert_eq!(redactor().text(line), line);
}

#[test]
fn a_second_pass_keeps_tokens_even_when_a_share_name_matches_inside_them() {
    let r = redactor();
    let once = r.text(r"\\nas\d\Plan.txt and D:\x\y.txt");
    assert_eq!(r.text(&once), once);
}

#[test]
fn a_trailing_apostrophe_inside_a_file_name_does_not_end_the_path() {
    let out = redactor().text(r"failed D:\Clients\Owners' Meeting.pdf");
    assert_absent(&out, &["owners", "meeting"]);
}

#[test]
fn a_trailing_apostrophe_in_a_folder_does_not_end_the_path() {
    let out = redactor().text(r"failed D:\Clients\Smiths'\Secret Plan.pdf");
    assert_absent(&out, &["smiths", "secret", "plan"]);
}

#[test]
fn a_trailing_apostrophe_in_a_unix_folder_does_not_end_the_path() {
    let out = redactor().text("open /mnt/data/Parents' Evening/Secret.pdf");
    assert_absent(&out, &["parents", "evening", "secret"]);
}

#[test]
fn a_quoted_path_ends_at_the_last_quote() {
    let out = redactor().text(r"'C:\x\Owners' Meeting.pdf'");
    assert_absent(&out, &["owners", "meeting"]);
}

#[test]
fn two_quoted_paths_leak_nothing() {
    let out = redactor().text(r"'D:\a\Secret.txt' and 'D:\b\Plan.txt'");
    assert_absent(&out, &["secret", "plan"]);
}

#[test]
fn a_triple_slash_path_is_found() {
    let out = redactor().text("x ///srv/Secret Plan.pdf");
    assert_absent(&out, &["srv", "secret", "plan"]);
}

#[test]
fn a_share_name_next_to_a_replaced_name_is_stable() {
    let r = redactor();
    let _ = file(&r, r"\\fs01\plan\a.txt");
    let once = r.text("Planadmin");
    assert_eq!(r.text(&once), once);
}

#[test]
fn a_drive_path_right_after_a_replaced_name_is_found() {
    let out = redactor().text("adminc:/Secret/Plan.txt");
    assert_absent(&out, &["secret", "plan"]);
}

#[test]
fn a_backslash_drive_path_after_a_lowercase_word_is_found() {
    let out = redactor().text(r"xc:\Secret\Plan.txt");
    assert_absent(&out, &["secret", "plan"]);
}

#[test]
fn a_forward_slash_drive_after_a_lowercase_word_is_not_a_path() {
    let line = "see http:/example";
    assert_eq!(redactor().text(line), line);
}

#[test]
fn long_text_with_many_quotes_is_redacted_quickly() {
    let line = "('a".repeat(30_000);
    let started = std::time::Instant::now();
    let _ = redactor().text(&line);
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
}

fn quick(line: &str) {
    let started = std::time::Instant::now();
    let _ = redactor().text(line);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "{}",
        line.len()
    );
}

#[test]
fn many_drive_shapes_are_redacted_quickly() {
    quick(&"xa:/".repeat(80_000));
}

#[test]
fn many_open_angle_brackets_are_redacted_quickly() {
    quick(&"<".repeat(320_000));
}

#[test]
fn a_forward_slash_drive_after_a_word_with_more_separators_is_found() {
    for line in ["filec:/a/Secret.txt", "xc:/Secret/Plan.txt"] {
        assert_absent(&redactor().text(line), &["secret", "plan"]);
    }
}

#[test]
fn an_unclosed_quote_before_a_name_with_a_trailing_apostrophe_leaks_nothing() {
    for line in [
        r"cannot rename 'D:\a\Owners' Meeting.pdf",
        r"'\\fs01\fin\Owners' Meeting.pdf",
    ] {
        assert_absent(&redactor().text(line), &["owners", "meeting"]);
    }
}

#[test]
fn paths_after_a_colon_with_more_separators_are_found() {
    for line in [
        "path:/mnt/Secret/Plan.txt",
        "error:/home/admin/Secret Plan.txt",
        "backup@nas:/srv/Secret/Plan.txt",
    ] {
        assert_absent(&redactor().text(line), &["secret", "plan"]);
    }
}

#[test]
fn file_and_smb_urls_are_paths() {
    for line in [
        "file:///home/admin/Secret.txt",
        "file://nas/finance/Secret.txt",
        "smb://nas/finance/Secret.txt",
    ] {
        assert_absent(&redactor().text(line), &["finance", "secret"]);
    }
}

#[test]
fn home_and_relative_paths_are_found() {
    for line in [
        "open ~/Secret/Plan.txt",
        "./Secret/Plan.txt",
        "x ../Secret/Plan.txt",
    ] {
        assert_absent(&redactor().text(line), &["secret", "plan"]);
    }
}

#[test]
fn paths_after_markup_and_pipes_are_found() {
    for line in ["<b>/mnt/Secret/Plan.txt", "a|/mnt/Secret/Plan.txt"] {
        assert_absent(&redactor().text(line), &["secret", "plan"]);
    }
}
