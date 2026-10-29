use std::fmt::Write as _;
use std::sync::atomic::AtomicBool;

use rusty_automotive::map::{MapModel, MapProblem, index_map};

mod words;

#[allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]
fn parse(body: &str) -> MapModel {
    let text = format!("TASKING VX-toolset linker map\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn line(cells: &[(&str, usize)]) -> String {
    let mut text = String::new();
    for (cell, width) in cells {
        text.push('|');
        if cell.len() > *width {
            assert!(write!(text, " {cell}").is_ok());
        } else {
            assert!(write!(text, " {cell:<width$} ").is_ok());
        }
    }
    text.push_str("|\n");
    text
}

const LINK: [usize; 6] = [12, 28, 15, 12, 36, 15];
const LINK_HEAD: &str = "* Link Result\n| [in] File | [in] Section | [in] Size (MAU) | [out] Offset | [out] Section | [out] Size (MAU) |\n";
const SEPARATOR: &str = "|----------------------------------------------------------|\n";

fn link(cells: [&str; 6]) -> String {
    line(
        &cells
            .iter()
            .zip(LINK)
            .map(|(c, w)| (*c, w))
            .collect::<Vec<_>>(),
    )
}

const LOCATE: [usize; 7] = [10, 8, 30, 12, 12, 12, 9];
const LOCATE_HEAD: &str = "* Locate Result\n+ Space orbit:core:linear (MAU = 8bit)\n| Chip | Group | Section | Size (MAU) | Space addr | Chip addr | Alignment |\n";

fn locate(cells: [&str; 7]) -> String {
    line(
        &cells
            .iter()
            .zip(LOCATE)
            .map(|(c, w)| (*c, w))
            .collect::<Vec<_>>(),
    )
}

const REMOVED: [usize; 4] = [34, 12, 6, 22];
const REMOVED_HEAD: &str = "* Removed Sections\n| Section | File | Library | Symbol |\n";

fn removed(cells: [&str; 4]) -> String {
    line(
        &cells
            .iter()
            .zip(REMOVED)
            .map(|(c, w)| (*c, w))
            .collect::<Vec<_>>(),
    )
}

const LONG_INPUT: &str = ".bss.Orbit_Far.SavedMemoryAddress (2291)";
const LONG_OUTPUT: &str = ".bss.Orbit_Far.SavedMemoryAddress (2291)";

fn wrapped_link_row() -> String {
    let (input, output) = (&LONG_INPUT[..28], &LONG_OUTPUT[..36]);
    let mut body = link([
        "orbit_far.o",
        input,
        "0x00000004",
        "0x0",
        output,
        "0x00000004",
    ]);
    body += &link(["", &LONG_INPUT[28..], "", "", &LONG_OUTPUT[36..], ""]);
    body
}

#[test]
fn link_result_joins_wrapped_names() {
    let body = format!("{LINK_HEAD}{}{SEPARATOR}", wrapped_link_row());
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions.len(), 1);
    let part = &model.contributions[0];
    assert_eq!(part.object_file, "orbit_far.o");
    assert_eq!(part.section, LONG_INPUT);
    assert_eq!(part.output_section, LONG_OUTPUT);
    assert_eq!((part.size, part.offset), (4, 0));
}

#[test]
fn a_cell_wrapping_over_two_continuation_lines_is_joined() {
    let name = ".text.Orbit_Far.Very_Long_Function_Name_For_Wrapping (4711)";
    let mut body = LINK_HEAD.to_string();
    body += &link([
        "orbit_far.o",
        &name[..28],
        "0x00000010",
        "0x0",
        ".text",
        "0x00000010",
    ]);
    body += &link(["", &name[28..56], "", "", "", ""]);
    body += &link(["", &name[56..], "", "", "", ""]);
    body += SEPARATOR;
    body += &link([
        "",
        ".text.next",
        "0x00000004",
        "0x10",
        ".text",
        "0x00000014",
    ]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions.len(), 2);
    assert_eq!(model.contributions[0].section, name);
    assert_eq!(model.contributions[1].object_file, "orbit_far.o");
}

#[test]
fn locate_result_joins_wrapped_section_names() {
    let name = ".text.Orbit_Far.Burn_Duration_Calculation (88)";
    let mut body = LOCATE_HEAD.to_string();
    body += &locate([
        "orbit:flash",
        "code",
        &name[..30],
        "0x00000020",
        "0x80000000",
        "0x0",
        "0x4",
    ]);
    body += &locate(["", "", &name[30..], "", "", "", ""]);
    body += &locate([
        "orbit:flash",
        "code",
        ".text.next (9)",
        "0x00000010",
        "0x80000020",
        "0x20",
        "0x4",
    ]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.sections.len(), 2);
    assert_eq!(model.sections[0].name, name);
    assert_eq!(
        (model.sections[0].address, model.sections[0].size),
        (0x8000_0000, 32)
    );
}

#[test]
fn processed_files_wrapped_rows_produce_no_problems() {
    let mut body = String::from(
        "* Processed Files\n| File | From archive | Symbol causing the extraction |\n",
    );
    body += &line(&[
        ("orbit_far_with_a_long_name.o", 14),
        ("libsupport.a", 12),
        ("Orbit_Far_Symbol", 16),
    ]);
    body += &line(&[("", 14), ("", 12), ("", 16)]);
    body += &line(&[
        ("orbit_far_wi", 12),
        ("libsupport.a", 12),
        ("Orbit_Far_Symbol_", 17),
    ]);
    body += &line(&[("th_a_long_name.o", 16), ("", 12), ("Name", 17)]);
    write!(body, "{LINK_HEAD}{}", wrapped_link_row()).unwrap();
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions.len(), 1);
}

#[test]
fn removed_sections_without_separators_join_wrapped_rows() {
    let first = ".text.Mod_Long.Mod_Long_BOOT_FAILURE (767)";
    let symbol = "Mod_Long_BOOT_FAILURE";
    let mut body = REMOVED_HEAD.to_string();
    body += &removed([&first[..34], "mod_long.o", "", symbol]);
    body += &removed([&first[34..], "", "", ""]);
    body += &removed([".text.plain (1)", "plain.o", "", "plain"]);
    body += &removed([&first[..34], "mod_long.o", "", symbol]);
    body += &removed([&first[34..], "", "", ""]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    let names: Vec<_> = model
        .discarded
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, [first, ".text.plain (1)", first]);
    assert_eq!(model.discarded[0].symbol.as_deref(), Some(symbol));
    assert_eq!(model.discarded[1].object_file.as_deref(), Some("plain.o"));
}

#[test]
fn a_wrapped_last_column_is_joined() {
    let symbol = "Mod_Long_Boot_Failure_Handler_Entry";
    let mut body = REMOVED_HEAD.to_string();
    body += &removed([".text.a (1)", "a.o", "lib.a", &symbol[..22]]);
    body += &removed(["", "", "", &symbol[22..]]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.discarded.len(), 1);
    assert_eq!(model.discarded[0].symbol.as_deref(), Some(symbol));
    assert_eq!(model.discarded[0].library.as_deref(), Some("lib.a"));
}

#[test]
fn an_exactly_filled_numeric_column_does_not_swallow_the_next_row() {
    let mut body = LINK_HEAD.to_string();
    body += &line(&[
        ("orbit.o", 12),
        (".text.a", 28),
        ("0x00000004", 15),
        ("0x0", 12),
        (".text", 36),
        ("0x00000004", 10),
    ]);
    body += &line(&[
        ("orbit.o", 12),
        (".text.b", 28),
        ("0x00000004", 10),
        ("0x4", 12),
        (".text", 36),
        ("0x00000008", 15),
    ]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions.len(), 2);
}

#[test]
fn a_filled_row_before_a_separator_or_the_end_is_kept() {
    let row = link([
        "orbit.o",
        ".text.a",
        "0x00000004",
        "0x0",
        ".text",
        "0x00000004",
    ]);
    let filled = line(&[
        ("orbit.o", 7),
        (".text.a", 7),
        ("0x00000004", 10),
        ("0x0", 3),
        (".text", 5),
        ("0x00000004", 10),
    ]);
    for body in [
        format!("{LINK_HEAD}{filled}{SEPARATOR}{row}"),
        format!("{LINK_HEAD}{row}{filled}"),
    ] {
        let model = parse(&body);
        assert!(model.problems.is_empty(), "{:?}", model.problems);
        assert_eq!(model.contributions.len(), 2);
    }
}

#[test]
fn malformed_rows_are_still_reported_at_the_row_start() {
    let mut body = LINK_HEAD.to_string();
    body += &wrapped_link_row();
    body += &link([
        "orbit.o",
        ".text.a",
        "not-a-number",
        "0x0",
        ".text",
        "0x00000004",
    ]);
    body += &line(&[("orbit.o", 12), (".text.b", 28)]);
    body += &link(["", "", "", "", "", ""]);
    let model = parse(&body);
    assert_eq!(model.contributions.len(), 1);
    let lines: Vec<_> = model
        .problems
        .iter()
        .filter_map(|problem| match problem {
            MapProblem::Malformed { line, .. } => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(lines, [6, 7], "{:?}", model.problems);
}

#[test]
fn a_wrapped_row_with_a_bad_number_is_reported_once() {
    let mut body = LOCATE_HEAD.to_string();
    body += &locate([
        "orbit:flash",
        "code",
        &".text.Orbit_Far.Burn_Duration_Calculation"[..30],
        "0xZZ",
        "0x80000000",
        "0x0",
        "0x4",
    ]);
    body += &locate(["", "", "n_Calculation", "", "", "", ""]);
    let model = parse(&body);
    assert!(model.sections.is_empty());
    assert_eq!(model.problem_total, 1, "{:?}", model.problems);
}
