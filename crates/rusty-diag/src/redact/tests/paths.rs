use super::{
    DriveKind, Identity, PathKind, Redactor, SALT_A, SALT_B, Salt, assert_absent, file, folder,
    redactor, segment_tokens, shape,
};
use crate::LengthClass;

fn plain() -> Redactor {
    Redactor::new(Salt::from_bytes(SALT_A), Identity::default())
}

fn unix_profile() -> Redactor {
    let identity = Identity {
        user: Some("admin".to_owned()),
        profile: Some("/home/admin".to_owned()),
        ..Identity::default()
    };
    Redactor::new(Salt::from_bytes(SALT_A), identity)
}

#[test]
fn drive_path_under_the_profile_becomes_user_root() {
    let out = file(&redactor(), r"C:\Users\admin\Pictures\holiday.jpg");
    assert_eq!(shape(out.as_str()), r"C:\<user>\<d>\<f>.jpg");
}

#[test]
fn profile_matches_drive_paths_case_insensitively() {
    let out = file(&redactor(), r"c:\USERS\Admin\Pictures\holiday.jpg");
    assert_eq!(shape(out.as_str()), r"C:\<user>\<d>\<f>.jpg");
}

#[test]
fn profile_matches_whole_segments_only() {
    let out = file(&redactor(), r"C:\Users\administrator\notes.txt");
    assert_eq!(shape(out.as_str()), r"C:\<d>\<d>\<f>.txt");
}

#[test]
fn profile_alone_becomes_user_root() {
    let out = folder(&redactor(), r"C:\Users\admin");
    assert_eq!(out.as_str(), r"C:\<user>");
}

#[test]
fn drive_path_without_profile_hashes_every_segment() {
    let out = file(&redactor(), r"D:\Projects\rusty\Cargo.toml");
    assert_eq!(shape(out.as_str()), r"D:\<d>\<d>\<f>.toml");
}

#[test]
fn lowercase_drive_letter_and_slashes_become_uppercase_and_backslashes() {
    let out = file(&plain(), "c:/Projects/rusty/Cargo.toml");
    assert_eq!(shape(out.as_str()), r"C:\<d>\<d>\<f>.toml");
}

#[test]
fn mixed_separators_in_a_drive_path_are_all_separators() {
    let out = file(&plain(), r"C:/Projects\rusty/Cargo.toml");
    assert_eq!(shape(out.as_str()), r"C:\<d>\<d>\<f>.toml");
}

#[test]
fn folder_last_segment_is_a_folder_token_without_extension() {
    let out = folder(&plain(), r"C:\Projects\release.v2");
    assert_eq!(shape(out.as_str()), r"C:\<d>\<d>");
}

#[test]
fn file_last_segment_is_a_file_token() {
    let out = file(&plain(), r"C:\Projects\release.zip");
    assert_eq!(shape(out.as_str()), r"C:\<d>\<f>.zip");
}

#[test]
fn extension_rules() {
    let r = plain();
    let cases = [
        (r"C:\a\photo.JPG", r"C:\<d>\<f>.jpg"),
        (r"C:\a\backup.tar.gz", r"C:\<d>\<f>.gz"),
        (r"C:\a\data.verylongext", r"C:\<d>\<f>"),
        (r"C:\a\file.abcdefgh", r"C:\<d>\<f>"),
        (r"C:\a\release.v2", r"C:\<d>\<f>"),
        (r"C:\a\file.abcdefghi", r"C:\<d>\<f>"),
        ("C:\\a\\note.\u{e9}", r"C:\<d>\<f>"),
        (r"C:\a\note.t-t", r"C:\<d>\<f>"),
        (r"C:\a\trailing.", r"C:\<d>\<f>"),
        (r"C:\a\noext", r"C:\<d>\<f>"),
    ];
    for (input, expected) in cases {
        assert_eq!(shape(file(&r, input).as_str()), expected, "{input}");
    }
}

#[test]
fn file_token_does_not_leak_the_stem_through_the_extension() {
    let out = file(&plain(), r"C:\a\secret.report.pdf");
    assert_absent(out.as_str(), &["secret", "report"]);
}

#[test]
fn unc_servers_and_shares_are_numbered_in_order_of_first_appearance() {
    let r = plain();
    let first = file(&r, r"\\NAS\Photos\a.jpg");
    let second = file(&r, r"\\backup\Docs\b.txt");
    let again = file(&r, r"\\nas\PHOTOS\c.jpg");
    let shapes = [first, second, again].map(|p| shape(p.as_str()));
    assert_eq!(
        shapes,
        [
            r"\\<server:1>\<share:1>\<f>.jpg".to_owned(),
            r"\\<server:2>\<share:2>\<f>.txt".to_owned(),
            r"\\<server:1>\<share:1>\<f>.jpg".to_owned(),
        ]
    );
}

#[test]
fn unc_numbering_restarts_in_a_new_redactor() {
    let _ = file(&plain(), r"\\NAS\Photos\a.jpg");
    let out = file(&plain(), r"\\backup\Docs\b.txt");
    assert_eq!(shape(out.as_str()), r"\\<server:1>\<share:1>\<f>.txt");
}

#[test]
fn long_path_prefix_is_dropped() {
    let out = file(&plain(), r"\\?\C:\Data\archive.bin");
    assert_eq!(shape(out.as_str()), r"C:\<d>\<f>.bin");
}

#[test]
fn long_path_prefix_sets_is_long() {
    assert!(file(&plain(), r"\\?\C:\Data\archive.bin").is_long());
}

#[test]
fn long_unc_prefix_hides_server_and_share() {
    let out = file(&plain(), r"\\?\UNC\fileserver\teamshare\plan.xlsx");
    assert_absent(out.as_str(), &["fileserver", "teamshare", "plan", "unc"]);
}

#[test]
fn unix_path_keeps_slash_root_and_separators() {
    let out = file(&plain(), "/srv/media/film.mkv");
    assert_eq!(shape(out.as_str()), "/<d>/<d>/<f>.mkv");
}

#[test]
fn unix_profile_becomes_user_root() {
    let out = file(&unix_profile(), "/home/admin/notes/todo.md");
    assert_eq!(shape(out.as_str()), "/<user>/<d>/<f>.md");
}

#[test]
fn unix_profile_match_is_case_sensitive() {
    let out = file(&unix_profile(), "/home/Admin/todo.md");
    assert_eq!(shape(out.as_str()), "/<d>/<d>/<f>.md");
}

#[test]
fn doubled_separators_are_dropped() {
    let r = plain();
    assert_eq!(
        shape(file(&r, r"C:\\Data\\\\photos\a.jpg").as_str()),
        r"C:\<d>\<d>\<f>.jpg"
    );
    assert_eq!(
        shape(file(&r, "/srv//media///a.mkv").as_str()),
        "/<d>/<d>/<f>.mkv"
    );
}

#[test]
fn same_name_in_any_case_gives_the_same_token() {
    let out = folder(&plain(), r"C:\Photos\PHOTOS\photos");
    let tokens = segment_tokens(out.as_str());
    assert_eq!(tokens.len(), 3, "{out}");
    assert!(tokens.iter().all(|t| *t == tokens[0]), "{out}");
}

#[test]
fn same_path_gives_the_same_output_across_calls_and_redactors() {
    let a = file(&plain(), r"C:\Photos\2026\Beach.JPG");
    let b = file(&plain(), r"c:\photos\2026\beach.jpg");
    assert_eq!(a.as_str(), b.as_str());
}

#[test]
fn different_salt_gives_different_tokens() {
    let path = r"C:\alpha\bravo\charlie\delta";
    let a = folder(&plain(), path);
    let b = folder(
        &Redactor::new(Salt::from_bytes(SALT_B), Identity::default()),
        path,
    );
    assert_ne!(a.as_str(), b.as_str());
}

#[test]
fn random_salts_differ() {
    let path = r"C:\alpha\bravo\charlie\delta";
    let a = folder(&Redactor::new(Salt::random(), Identity::default()), path);
    let b = folder(&Redactor::new(Salt::random(), Identity::default()), path);
    assert_ne!(a.as_str(), b.as_str());
}

#[test]
fn depth_counts_segments_after_the_root() {
    let r = plain();
    let cases = [
        (r"C:\", 0),
        (r"C:\a", 1),
        (r"C:\a\b\c.txt", 3),
        (r"C:\Users\admin\a.txt", 3),
        (r"\\srv\share", 0),
        (r"\\srv\share\a\b", 2),
        ("/", 0),
        ("/a/b", 2),
        (r"a\b", 2),
    ];
    for (input, depth) in cases {
        assert_eq!(file(&r, input).depth(), depth, "{input}");
    }
}

#[test]
fn depth_of_a_profile_path_counts_the_original_segments() {
    assert_eq!(file(&redactor(), r"C:\Users\admin\a\b.txt").depth(), 4);
}

fn path_of_length(len: usize) -> String {
    let mut path = String::from(r"C:\");
    while path.chars().count() + 11 < len {
        path.push_str(r"abcdefghij\");
    }
    while path.chars().count() < len {
        path.push('x');
    }
    path
}

#[test]
fn length_class_boundaries() {
    let r = plain();
    let cases = [
        (63, LengthClass::Short),
        (64, LengthClass::Medium),
        (259, LengthClass::Medium),
        (260, LengthClass::Long),
    ];
    for (len, class) in cases {
        let path = path_of_length(len);
        assert_eq!(path.chars().count(), len);
        assert_eq!(file(&r, &path).length_class(), class, "length {len}");
    }
}

#[test]
fn length_counts_characters_not_bytes() {
    let path = format!(r"C:\{}", "\u{e9}".repeat(60));
    assert_eq!(file(&plain(), &path).length_class(), LengthClass::Short);
}

#[test]
fn is_long_from_260_characters() {
    let r = plain();
    assert_eq!(
        [259, 260].map(|len| file(&r, &path_of_length(len)).is_long()),
        [false, true]
    );
}

#[test]
fn non_ascii_flag() {
    let r = plain();
    assert_eq!(
        [r"C:\Fotos\ete.jpg", "C:\\Fotos\\\u{e9}t\u{e9}.jpg"].map(|p| file(&r, p).has_non_ascii()),
        [false, true]
    );
}

#[test]
fn drive_kind_is_kept() {
    let r = plain();
    let kinds = [
        DriveKind::Local,
        DriveKind::Network,
        DriveKind::Removable,
        DriveKind::Unknown,
    ];
    for kind in kinds {
        assert_eq!(r.path(r"E:\a.txt", PathKind::File, kind).drive(), kind);
    }
}

#[test]
fn display_matches_as_str() {
    let out = file(&plain(), r"C:\a\b.txt");
    assert_eq!(out.to_string(), out.as_str());
}

#[test]
fn empty_string_is_an_empty_shallow_path() {
    let out = file(&plain(), "");
    assert_eq!(
        (out.as_str(), out.depth(), out.length_class(), out.is_long()),
        ("", 0, LengthClass::Short, false)
    );
}

#[test]
fn lone_drive_letter_is_the_drive_root() {
    let out = folder(&plain(), "C:");
    assert_eq!((out.as_str(), out.depth()), (r"C:\", 0));
}

#[test]
fn relative_path_hides_its_segments() {
    let out = file(&plain(), r"Documents\taxes\return.pdf");
    assert_absent(out.as_str(), &["documents", "taxes", "return"]);
}

#[test]
fn malformed_unc_hides_the_server() {
    let r = plain();
    for input in [
        r"\\lonelyserver",
        r"\\lonelyserver\",
        r"\\\lonelyserver\\share",
    ] {
        assert_absent(file(&r, input).as_str(), &["lonelyserver"]);
    }
}

#[test]
fn a_name_after_the_last_dot_that_is_no_known_extension_is_hidden() {
    let out = file(&plain(), r"D:\Contacts\Jane.Doe");
    assert_absent(out.as_str(), &["doe"]);
}

#[test]
fn a_known_extension_in_upper_case_is_kept_in_lower_case() {
    let out = file(&plain(), r"D:\a\report.PDF");
    assert_eq!(shape(out.as_str()), r"D:\<d>\<f>.pdf");
}

#[test]
fn forward_slash_unc_path_is_read_as_unc() {
    let out = file(&plain(), "//server/share/x.txt");
    assert_eq!(shape(out.as_str()), r"\\<server:1>\<share:1>\<f>.txt");
}
