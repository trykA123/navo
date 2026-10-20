use super::*;

#[test]
fn a_cell_wrapped_at_a_word_boundary_joins_with_a_space() {
    let mut body = LINK_HEAD.to_string();
    body += &link([
        "orbit_far.o",
        ".bss..section_bootstatus",
        "0x00000008",
        "0x0",
        ".bss..section_bootstatus",
        "0x00000008",
    ]);
    body += &link(["", "(2365)", "", "", "(2365)", ""]);
    body += SEPARATOR;
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions.len(), 1);
    let part = &model.contributions[0];
    assert_eq!(part.section, ".bss..section_bootstatus (2365)");
    assert_eq!(part.output_section, ".bss..section_bootstatus (2365)");
}

#[test]
fn a_cell_wrapped_inside_a_word_joins_without_a_space() {
    let name = ".bss.Orbit_Far.SavedMemoryAddress (2291)";
    let mut body = LINK_HEAD.to_string();
    body += &link([
        "orbit_far.o",
        &name[..28],
        "0x00000004",
        "0x0",
        ".bss",
        "0x00000004",
    ]);
    body += &link(["", &name[28..], "", "", "", ""]);
    body += SEPARATOR;
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions[0].section, name);
}

#[test]
fn locate_result_cell_wrapping_over_two_continuation_lines_is_joined() {
    let name = ".data.Orbit_Far.Burn_Key_Info_Random_Seed_State (176) S_CBC (6909)";
    let mut body = LOCATE_HEAD.to_string();
    body += &locate([
        "orbit:ram",
        "data",
        &name[..30],
        "0x00000001",
        "0x70002288",
        "0x2088",
        "0x8",
    ]);
    body += &locate(["", "", &name[30..60], "", "", "", ""]);
    body += &locate(["", "", &name[60..], "", "", "", ""]);
    body += &locate([
        "orbit:ram",
        "data",
        ".data.next (1)",
        "0x00000004",
        "0x70002290",
        "0x2090",
        "0x4",
    ]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.sections.len(), 2);
    assert_eq!(model.sections[0].name, name);
    assert_eq!(model.sections[1].name, ".data.next (1)");
}

#[test]
fn removed_sections_continuation_with_a_first_cell_joins_a_word_wrap() {
    let mut body = REMOVED_HEAD.to_string();
    body += &removed([".text.Mod_Long", "mod_long.o", "", "Mod_Long_BOOT"]);
    body += &removed(["BOOT_FAILURE (767)", "", "", "FAILURE"]);
    body += &removed([".text.plain (1)", "plain.o", "", "plain"]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.discarded.len(), 2);
    assert_eq!(model.discarded[0].name, ".text.Mod_Long BOOT_FAILURE (767)");
    assert_eq!(
        model.discarded[0].symbol.as_deref(),
        Some("Mod_Long_BOOT FAILURE")
    );
    assert_eq!(model.discarded[1].name, ".text.plain (1)");
}

#[test]
fn a_full_row_with_an_exactly_filled_text_cell_does_not_merge_with_the_next_row() {
    let full = ".text.Orbit_Far.Burn_Time_Calc";
    assert_eq!(full.len(), 30);
    let mut body = LOCATE_HEAD.to_string();
    for (name, address) in [(full, "0x80000000"), (".text.next (9)", "0x80000010")] {
        body += &locate([
            "orbit:flash",
            "code",
            name,
            "0x00000010",
            address,
            "0x0",
            "0x4",
        ]);
    }
    body += REMOVED_HEAD;
    let filled = format!(".text.{}", "a".repeat(28));
    body += &removed([&filled, "a.o", "", "a_sym"]);
    body += &removed(["plain", "b.o", "", "b_sym"]);
    let model = parse(&body);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.sections.len(), 2);
    assert_eq!(model.sections[0].name, full);
    assert_eq!(model.discarded.len(), 2);
}
