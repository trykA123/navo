use super::*;

#[test]
fn index_memory_is_a_standalone_console_command_with_optional_json() {
    for args in [&["--index-memory"][..], &["--index-memory", "--json"]] {
        let command = parse(args.iter().map(OsString::from)).unwrap();
        assert_eq!(command, Command::IndexMemory);
        assert!(command.uses_console());
    }
}

#[test]
fn index_memory_refuses_other_commands_flags_paths_and_duplicates() {
    for args in [
        &["--index-memory", "--json", "--json"][..],
        &["--index-memory", "--index-memory"],
        &["--index-memory", "--left", "folder"],
        &["--index-memory", "--version"],
        &["--index-memory", "folder"],
        &["folder", "--index-memory"],
        &["--left", "folder", "--index-memory"],
        &["--json", "--index-memory"],
    ] {
        assert!(parse(args.iter().map(OsString::from)).is_err(), "{args:?}");
    }
}
