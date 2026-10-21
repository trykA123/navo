use crate::command_center::{Group, Heading, Hit, Results, Row, Scope, Selection, Target};

fn hit(source: &'static str, group: Group, title: &str, score: i64) -> Hit {
    Hit {
        source,
        group,
        title: title.to_owned().into(),
        detail: "".into(),
        score,
        indices: Vec::new(),
        target: Target::Action("window.close"),
        shortcut: None,
    }
}

fn many(group: Group, count: usize, top_score: i64) -> Vec<Hit> {
    (0..count)
        .map(|n| hit("src", group, &format!("{group:?}{n}"), top_score - n as i64))
        .collect()
}

fn titles(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Header(heading) => format!("# {heading:?}"),
            Row::TopHit(hit) => format!("top {}", hit.title),
            Row::Hit(hit) => hit.title.to_string(),
            Row::ShowAll { group, total } => format!("all {group:?} {total}"),
            Row::Hint => "hint".to_owned(),
        })
        .collect()
}

fn mixed(hits: Vec<Hit>) -> Vec<Row> {
    Results::merge(hits, Scope::Mixed)
}

#[test]
fn no_hits_make_no_rows() {
    assert!(mixed(Vec::new()).is_empty());
}

#[test]
fn top_hit_is_the_best_hit_across_groups() {
    let rows = mixed(vec![
        hit("a", Group::Commands, "Open", 40),
        hit("b", Group::Files, "open.txt", 90),
        hit("c", Group::Folders, "opt", 60),
    ]);
    assert_eq!(
        titles(&rows),
        [
            "# TopHit",
            "top open.txt",
            "# Group(Commands)",
            "Open",
            "# Group(Folders)",
            "opt",
        ]
    );
}

#[test]
fn top_hit_is_not_repeated_and_empty_groups_are_omitted() {
    let rows = mixed(vec![
        hit("a", Group::Commands, "Only", 40),
        hit("b", Group::Files, "x", 10),
    ]);
    assert_eq!(
        titles(&rows),
        ["# TopHit", "top Only", "# Group(Files)", "x"]
    );
}

#[test]
fn a_single_hit_leaves_only_the_top_hit() {
    let rows = mixed(vec![hit("a", Group::Files, "x", 10)]);
    assert_eq!(titles(&rows), ["# TopHit", "top x"]);
}

#[test]
fn groups_come_in_fixed_order_whatever_the_scores() {
    let rows = mixed(vec![
        hit("m", Group::Module("tabs"), "tab", 50),
        hit("f", Group::Files, "file", 60),
        hit("d", Group::Folders, "dir", 70),
        hit("c", Group::Commands, "cmd", 80),
        hit("c", Group::Commands, "cmd2", 5),
    ]);
    let headings: Vec<Heading> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Header(heading) => Some(heading.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        headings,
        [
            Heading::TopHit,
            Heading::Group(Group::Commands),
            Heading::Group(Group::Folders),
            Heading::Group(Group::Files),
            Heading::Group(Group::Module("tabs")),
        ]
    );
}

#[test]
fn hits_inside_a_group_are_sorted_by_score() {
    let rows = mixed(vec![
        hit("a", Group::Files, "low", 1),
        hit("a", Group::Files, "high", 9),
        hit("a", Group::Files, "mid", 5),
    ]);
    assert_eq!(
        titles(&rows),
        ["# TopHit", "top high", "# Group(Files)", "mid", "low"]
    );
}

#[test]
fn ties_keep_source_order_then_shorter_paths() {
    let mut late = hit("second", Group::Files, "same", 5);
    late.detail = "a".into();
    let mut long = hit("first", Group::Files, "same-long", 5);
    long.detail = "aaaa".into();
    let mut short = hit("first", Group::Files, "same-short", 5);
    short.detail = "a".into();
    let rows = mixed(vec![long, late, short]);
    assert_eq!(
        titles(&rows),
        [
            "# TopHit",
            "top same-short",
            "# Group(Files)",
            "same-long",
            "same"
        ]
    );
}

#[test]
fn mixed_mode_caps_each_group_and_ends_with_show_all() {
    let mut hits = many(Group::Commands, 9, 100);
    hits.extend(many(Group::Folders, 5, 50));
    hits.extend(many(Group::Files, 20, 40));
    let rows = mixed(hits);
    let names = titles(&rows);
    let count = |prefix: &str| {
        names
            .iter()
            .filter(|name| name.starts_with(prefix) && !name.starts_with("top"))
            .count()
    };
    assert_eq!(count("Commands"), 5);
    assert_eq!(count("Folders"), 5);
    assert_eq!(count("Files"), 8);
    assert!(names.contains(&"all Commands 9".to_owned()));
    assert!(!names.iter().any(|name| name.starts_with("all Folders")));
    assert!(names.contains(&"all Files 20".to_owned()));
}

#[test]
fn a_group_exactly_at_its_cap_has_no_show_all_row() {
    let rows = mixed(many(Group::Commands, 6, 100));
    assert!(!titles(&rows).iter().any(|name| name.starts_with("all")));
}

#[test]
fn show_all_counts_the_group_total_with_the_top_hit() {
    let rows = mixed(many(Group::Commands, 12, 100));
    assert!(titles(&rows).contains(&"all Commands 12".to_owned()));
}

#[test]
fn scoped_mode_shows_one_group_up_to_500_rows_without_top_hit() {
    let rows = Results::merge(many(Group::Files, 700, 1_000), Scope::Files);
    let names = titles(&rows);
    assert_eq!(names[0], "# Group(Files)");
    assert_eq!(rows.len(), 501);
    assert!(!names.iter().any(|name| name.starts_with("all")));
}

#[test]
fn scoped_commands_list_every_command_hit() {
    let rows = Results::merge(many(Group::Commands, 40, 100), Scope::Commands);
    assert_eq!(rows.len(), 41);
}

#[test]
fn recent_lists_five_commands_and_eight_folders_then_the_hint() {
    let rows = Results::recent(many(Group::Commands, 7, 10), many(Group::Folders, 12, 10));
    let names = titles(&rows);
    assert_eq!(names[0], "# Recent");
    assert_eq!(rows.len(), 1 + 5 + 8 + 1);
    assert_eq!(names[1], "Commands0");
    assert_eq!(names[6], "Folders0");
    assert_eq!(names.last().map(String::as_str), Some("hint"));
}

#[test]
fn recent_without_history_is_only_the_hint() {
    let rows = Results::recent(Vec::new(), Vec::new());
    assert_eq!(rows, [Row::Hint]);
}

fn sample_rows() -> Vec<Row> {
    let mut hits = many(Group::Commands, 3, 100);
    hits.extend(many(Group::Folders, 2, 50));
    hits.extend(many(Group::Files, 2, 40));
    mixed(hits)
}

fn selected_title(rows: &[Row], selection: Selection) -> String {
    match selection.row(rows) {
        Some(Row::TopHit(hit) | Row::Hit(hit)) => hit.title.to_string(),
        Some(Row::ShowAll { group, total }) => format!("all {group:?} {total}"),
        other => format!("{other:?}"),
    }
}

#[test]
fn selection_starts_on_the_first_selectable_row() {
    let rows = sample_rows();
    assert_eq!(selected_title(&rows, Selection::start(&rows)), "Commands0");
}

#[test]
fn selection_on_no_rows_is_empty() {
    let selection = Selection::start(&[]);
    assert_eq!(selection.row(&[]), None);
    let mut selection = selection;
    selection.step(&[], 1);
    selection.last(&[]);
    assert_eq!(selection.row(&[]), None);
}

#[test]
fn selection_on_only_a_hint_selects_nothing() {
    let rows = [Row::Hint];
    assert_eq!(Selection::start(&rows).row(&rows), None);
}

#[test]
fn step_skips_headers_and_clamps_at_both_ends() {
    let rows = sample_rows();
    let mut selection = Selection::start(&rows);
    selection.step(&rows, -1);
    assert_eq!(selected_title(&rows, selection), "Commands0");
    selection.step(&rows, 1);
    assert_eq!(selected_title(&rows, selection), "Commands1");
    selection.step(&rows, 2);
    assert_eq!(selected_title(&rows, selection), "Folders0");
    selection.step(&rows, 100);
    assert_eq!(selected_title(&rows, selection), "Files1");
    selection.step(&rows, 1);
    assert_eq!(selected_title(&rows, selection), "Files1");
}

#[test]
fn page_moves_by_whole_selectable_rows() {
    let rows = sample_rows();
    let mut selection = Selection::start(&rows);
    selection.step(&rows, 4);
    assert_eq!(selected_title(&rows, selection), "Folders1");
    selection.step(&rows, -4);
    assert_eq!(selected_title(&rows, selection), "Commands0");
}

#[test]
fn first_and_last_jump_to_the_ends() {
    let rows = sample_rows();
    let mut selection = Selection::start(&rows);
    selection.last(&rows);
    assert_eq!(selected_title(&rows, selection), "Files1");
    selection.first(&rows);
    assert_eq!(selected_title(&rows, selection), "Commands0");
}

#[test]
fn tab_walks_the_groups_skipping_headers_and_wraps() {
    let rows = sample_rows();
    let mut selection = Selection::start(&rows);
    selection.next_group(&rows);
    assert_eq!(selected_title(&rows, selection), "Commands1");
    selection.next_group(&rows);
    assert_eq!(selected_title(&rows, selection), "Folders0");
    selection.next_group(&rows);
    assert_eq!(selected_title(&rows, selection), "Files0");
    selection.next_group(&rows);
    assert_eq!(selected_title(&rows, selection), "Commands0");
}

#[test]
fn shift_tab_walks_back_and_wraps() {
    let rows = sample_rows();
    let mut selection = Selection::start(&rows);
    selection.previous_group(&rows);
    assert_eq!(selected_title(&rows, selection), "Files0");
    selection.previous_group(&rows);
    assert_eq!(selected_title(&rows, selection), "Folders0");
}

#[test]
fn the_top_hit_is_its_own_stop_for_tab() {
    let rows = mixed(vec![
        hit("a", Group::Commands, "cmd", 90),
        hit("a", Group::Commands, "cmd2", 80),
        hit("a", Group::Files, "file", 10),
    ]);
    let mut selection = Selection::start(&rows);
    assert_eq!(selected_title(&rows, selection), "cmd");
    selection.next_group(&rows);
    assert_eq!(selected_title(&rows, selection), "cmd2");
    selection.next_group(&rows);
    assert_eq!(selected_title(&rows, selection), "file");
}

#[test]
fn show_all_rows_are_selectable() {
    let rows = mixed(many(Group::Commands, 12, 100));
    let mut selection = Selection::start(&rows);
    selection.last(&rows);
    assert_eq!(selected_title(&rows, selection), "all Commands 12");
}

fn place_hit(source: &'static str, path: &str, score: i64) -> Hit {
    let location = rusty_core::Location::new(path).unwrap();
    Hit {
        title: path.to_owned().into(),
        target: if source == "tabs" {
            Target::GoTo(location)
        } else {
            Target::Open {
                location,
                folder: true,
            }
        },
        ..hit(source, Group::Folders, path, score)
    }
}

#[test]
fn a_place_found_by_recent_and_tabs_shows_once_as_the_tabs_row_with_the_higher_score() {
    let hits = vec![
        place_hit("recent", "C:\\Work\\Ferrite", 90),
        place_hit("tabs", "c:\\work\\ferrite", 60),
        place_hit("tabs", "c:\\work\\ferrite", 60),
        place_hit("recent", "/work/other", 50),
    ];
    let rows = Results::merge(hits, Scope::Files);
    let shown: Vec<&Hit> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Hit(hit) => Some(hit),
            _ => None,
        })
        .collect();
    assert_eq!(shown.len(), 2, "{rows:?}");
    assert_eq!((shown[0].source, shown[0].score), ("tabs", 90));
    assert_eq!(shown[1].source, "recent");
}

#[test]
fn tabs_win_over_recent_and_recent_over_here_for_one_place() {
    let hits = vec![
        place_hit("here", "/work/a", 70),
        place_hit("recent", "/work/a", 50),
        place_hit("here", "/work/b", 40),
        place_hit("recent", "/work/b", 60),
        place_hit("here", "/work/c", 30),
        place_hit("tabs", "/work/c", 20),
        place_hit("recent", "/work/c", 10),
        place_hit("here", "/work/d", 5),
    ];
    let rows = Results::merge(hits, Scope::Files);
    let shown: Vec<(&str, &str, i64)> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Hit(hit) => Some((hit.source, hit.title.as_ref(), hit.score)),
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("recent", "/work/a", 70),
            ("recent", "/work/b", 60),
            ("tabs", "/work/c", 30),
            ("here", "/work/d", 5),
        ]
    );
}
