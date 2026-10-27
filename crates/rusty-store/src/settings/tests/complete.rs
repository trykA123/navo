use super::*;

const FULL: &str = "[appearance]
mode = \"system\"
color-theme = \"open-sea\"
light-theme = \"Navo Light\"
dark-theme = \"Navo Dark\"
font = \"Inter\"
font-size = 13
ui-scale = 100
density = \"normal\"
modifier-hints = \"all\"
modifier-hints-delay-ms = 400

[files]
hidden-files = \"shown\"
show-system = false
folders-first = true
sort = \"name\"
sort-descending = false
size-mode = \"apparent\"
measure-shown-folders = false
drop-confirm = \"never\"
rename-log = false

[selection]
space-measures-folders = true
patterns-select-folders = false

[terminal]
provider = \"windows-terminal\"

[window.view]
tabs-with-one = true
path-bar = true
drive-button = true
title-search = true
fkey-bar = false
icons = true
ext-column = true
size-column = true
date-column = true
free-space = true
marked-totals = true
contents = true
ext-width = 0
size-width = 0
date-width = 0

[text.view]
line-numbers = true
line-ends = false
syntax-theme = \"oxidised-iron\"

[image.view]
details = false

[markdown.view]
preview-split = true

[preview]
inspector = false
inspector-width = 360
inspector-properties = true

[preview.view]
type-bars = true
largest-files = true

[copy]
mode = \"auto\"

[retention]
retain-deletes = false
retain-overwrites = false
days = 3
max-volume-mib = 1024
min-free-mib = 256

[search]
index = \"auto\"
drives = []
walk-roots = []
exclude = [\"$Recycle.Bin\", \"System Volume Information\"]
network = false
removable = false
remember = true

[automotive]
modules = []

[automotive.view]
preset = \"simple\"
inspector = false
memory-strip = false
charts = false
row-bars = false
usage-bars = false
who-uses = false
";

const DEFAULT_KEYS: [(&str, &str); 68] = [
    ("appearance.mode", "\"system\""),
    ("appearance.color-theme", "\"open-sea\""),
    ("appearance.light-theme", "\"Navo Light\""),
    ("appearance.dark-theme", "\"Navo Dark\""),
    ("appearance.font", "\"Inter\""),
    ("appearance.font-size", "13"),
    ("appearance.ui-scale", "100"),
    ("appearance.density", "\"normal\""),
    ("appearance.modifier-hints", "\"all\""),
    ("appearance.modifier-hints-delay-ms", "400"),
    ("files.hidden-files", "\"shown\""),
    ("files.show-system", "false"),
    ("files.folders-first", "true"),
    ("files.sort", "\"name\""),
    ("files.sort-descending", "false"),
    ("files.size-mode", "\"apparent\""),
    ("files.measure-shown-folders", "false"),
    ("files.drop-confirm", "\"never\""),
    ("files.rename-log", "false"),
    ("selection.space-measures-folders", "true"),
    ("selection.patterns-select-folders", "false"),
    ("terminal.provider", "\"windows-terminal\""),
    ("window.view.tabs-with-one", "true"),
    ("window.view.path-bar", "true"),
    ("window.view.drive-button", "true"),
    ("window.view.title-search", "true"),
    ("window.view.fkey-bar", "false"),
    ("window.view.icons", "true"),
    ("window.view.ext-column", "true"),
    ("window.view.size-column", "true"),
    ("window.view.date-column", "true"),
    ("window.view.free-space", "true"),
    ("window.view.marked-totals", "true"),
    ("window.view.contents", "true"),
    ("window.view.ext-width", "0"),
    ("window.view.size-width", "0"),
    ("window.view.date-width", "0"),
    ("text.view.line-numbers", "true"),
    ("text.view.line-ends", "false"),
    ("text.view.syntax-theme", "\"oxidised-iron\""),
    ("image.view.details", "false"),
    ("markdown.view.preview-split", "true"),
    ("preview.inspector", "false"),
    ("preview.inspector-width", "360"),
    ("preview.inspector-properties", "true"),
    ("preview.view.type-bars", "true"),
    ("preview.view.largest-files", "true"),
    ("copy.mode", "\"auto\""),
    ("retention.retain-deletes", "false"),
    ("retention.retain-overwrites", "false"),
    ("retention.days", "3"),
    ("retention.max-volume-mib", "1024"),
    ("retention.min-free-mib", "256"),
    ("search.index", "\"auto\""),
    ("search.drives", "[]"),
    ("search.walk-roots", "[]"),
    (
        "search.exclude",
        "[\"$Recycle.Bin\", \"System Volume Information\"]",
    ),
    ("search.network", "false"),
    ("search.removable", "false"),
    ("search.remember", "true"),
    ("automotive.modules", "[]"),
    ("automotive.view.preset", "\"simple\""),
    ("automotive.view.inspector", "false"),
    ("automotive.view.memory-strip", "false"),
    ("automotive.view.charts", "false"),
    ("automotive.view.row-bars", "false"),
    ("automotive.view.usage-bars", "false"),
    ("automotive.view.who-uses", "false"),
];

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn ledger_of(list: &[(&str, &str)]) -> Ledger {
    pairs(list).into_iter().collect()
}

fn sorted_by_date() -> Settings {
    let mut settings = Settings::default();
    settings.files.sort = SortKey::Date;
    settings
}

fn completed(text: &str) -> String {
    complete_text(text, &Settings::default()).unwrap().text
}

fn unchanged_by_reconcile(text: &str) {
    let ledger = ledger_of(&DEFAULT_KEYS);
    let done = reconcile(text, &ledger, &Settings::default()).unwrap();
    assert_eq!(done.text.as_bytes(), text.as_bytes());
    assert_eq!(done.written, []);
    assert_eq!(done.changes, []);
}

#[test]
fn an_empty_file_gets_every_key_in_model_order() {
    let done = complete_text("", &Settings::default()).unwrap();
    assert_eq!(done.text, FULL);
    assert_eq!(done.written, pairs(&DEFAULT_KEYS));
    assert_eq!(done.changes, []);
}

#[test]
fn a_complete_file_is_left_as_it_is() {
    let done = complete_text(FULL, &Settings::default()).unwrap();
    assert_eq!(done.text, FULL);
    assert_eq!(done.written, []);
}

#[test]
fn missing_keys_join_their_section_and_missing_sections_follow_in_model_order() {
    let before =
        "# mine\n[files]\nsort = \"size\" # by size\n\n[appearance]\n# the look\nmode = \"dark\"\n";
    let after = "# mine\n[files]\nsort = \"size\" # by size\nhidden-files = \"shown\"\nshow-system = false\nfolders-first = true\nsort-descending = false\nsize-mode = \"apparent\"\nmeasure-shown-folders = false\ndrop-confirm = \"never\"\nrename-log = false\n\n[appearance]\n# the look\nmode = \"dark\"\ncolor-theme = \"open-sea\"\nlight-theme = \"Navo Light\"\ndark-theme = \"Navo Dark\"\nfont = \"Inter\"\nfont-size = 13\nui-scale = 100\ndensity = \"normal\"\nmodifier-hints = \"all\"\nmodifier-hints-delay-ms = 400\n\n[selection]\nspace-measures-folders = true\npatterns-select-folders = false\n\n[terminal]\nprovider = \"windows-terminal\"\n\n[window.view]\ntabs-with-one = true\npath-bar = true\ndrive-button = true\ntitle-search = true\nfkey-bar = false\nicons = true\next-column = true\nsize-column = true\ndate-column = true\nfree-space = true\nmarked-totals = true\ncontents = true\next-width = 0\nsize-width = 0\ndate-width = 0\n\n[text.view]\nline-numbers = true\nline-ends = false\nsyntax-theme = \"oxidised-iron\"\n\n[image.view]\ndetails = false\n\n[markdown.view]\npreview-split = true\n\n[preview]\ninspector = false\ninspector-width = 360\ninspector-properties = true\n\n[preview.view]\ntype-bars = true\nlargest-files = true\n\n[copy]\nmode = \"auto\"\n\n[retention]\nretain-deletes = false\nretain-overwrites = false\ndays = 3\nmax-volume-mib = 1024\nmin-free-mib = 256\n\n[search]\nindex = \"auto\"\ndrives = []\nwalk-roots = []\nexclude = [\"$Recycle.Bin\", \"System Volume Information\"]\nnetwork = false\nremovable = false\nremember = true\n\n[automotive]\nmodules = []\n\n[automotive.view]\npreset = \"simple\"\ninspector = false\nmemory-strip = false\ncharts = false\nrow-bars = false\nusage-bars = false\nwho-uses = false\n";
    assert_eq!(completed(before), after);
}

#[test]
fn only_the_missing_keys_are_reported_as_written() {
    let before = FULL
        .replace("font-size = 13\n", "")
        .replace("sort = \"name\"\n", "");
    let done = complete_text(&before, &Settings::default()).unwrap();
    assert_eq!(
        done.written,
        pairs(&[("appearance.font-size", "13"), ("files.sort", "\"name\"")])
    );
}

#[test]
fn existing_values_are_never_changed_even_when_they_differ_from_the_default() {
    let before = "[appearance]\nfont-size = 15\nmode = 'dark'\n";
    let after = completed(before);
    assert!(
        after.starts_with("[appearance]\nfont-size = 15\nmode = 'dark'\n"),
        "{after}"
    );
    let settings = parse(&after).unwrap().0;
    assert_eq!(settings.appearance.font_size, 15);
    assert_eq!(settings.appearance.mode, ThemeMode::Dark);
}

#[test]
fn comments_blank_lines_odd_spacing_and_unknown_keys_survive() {
    let before = "# top\n\n[appearance]\n  mode=\"dark\"   # night\n\n\n# between\nzoom = 2\n\n[plugins]\ngit = true # keep\n";
    let after = completed(before);
    for line in before.lines() {
        assert!(after.lines().any(|l| l == line), "lost {line:?} in {after}");
    }
    let kept: Vec<&str> = after
        .lines()
        .filter(|l| before.lines().any(|b| b == *l))
        .collect();
    let order: Vec<&str> = before.lines().collect();
    assert_eq!(
        kept.iter().filter(|l| !l.is_empty()).collect::<Vec<_>>(),
        order.iter().filter(|l| !l.is_empty()).collect::<Vec<_>>()
    );
    assert!(after.contains("[plugins]\ngit = true # keep\n"), "{after}");
}

#[test]
fn a_crlf_file_stays_crlf_and_keeps_its_lines() {
    let before = "# mine\r\n[files]\r\nsort = \"size\"\r\n";
    let after = completed(before);
    assert!(
        after.starts_with("# mine\r\n[files]\r\nsort = \"size\"\r\n"),
        "{after:?}"
    );
    assert_eq!(
        after.matches("\r\n").count(),
        after.matches('\n').count(),
        "{after:?}"
    );
}

#[test]
fn a_bom_file_keeps_its_bom() {
    let before = "\u{feff}[files]\nsort = \"size\"\n";
    let after = completed(before);
    assert!(
        after.starts_with("\u{feff}[files]\nsort = \"size\"\n"),
        "{after:?}"
    );
    assert_eq!(after.matches('\u{feff}').count(), 1);
}

#[test]
fn dotted_and_inline_sections_keep_their_values() {
    for before in [
        "appearance.mode = \"dark\"\n",
        "files = { sort = \"size\" }\n",
    ] {
        let after = completed(before);
        let settings = parse(&after).unwrap().0;
        assert_eq!(
            complete_text(&after, &Settings::default()).unwrap().written,
            [],
            "{after}"
        );
        let mut expected = Settings::default();
        if before.starts_with("appearance") {
            expected.appearance.mode = ThemeMode::Dark;
        } else {
            expected.files.sort = SortKey::Size;
        }
        assert_eq!(settings, expected, "{after}");
    }
}

#[test]
fn optional_and_experimental_keys_are_kept_and_not_added() {
    let before = "[terminal]\ncommand = \"wezterm\"\n\n[editor]\npath = \"code\"\n\n[experimental]\nfast-copy = true\n";
    let after = completed(before);
    assert!(after.contains("command = \"wezterm\"\n"), "{after}");
    assert!(after.contains("[editor]\npath = \"code\"\n"), "{after}");
    assert!(
        after.contains("[experimental]\nfast-copy = true\n"),
        "{after}"
    );
    assert!(!after.contains(UNUSED_NOTE), "{after}");
}

#[test]
fn a_file_that_does_not_parse_is_refused() {
    for broken in [
        "[appearance\nmode = \"dark\"\n",
        "[appearance]\nfont-size = \"big\"\n",
    ] {
        assert_eq!(
            complete_text(broken, &Settings::default()),
            Err(SettingsError::FileInvalid)
        );
        assert_eq!(
            reconcile(broken, &Ledger::new(), &Settings::default()),
            Err(SettingsError::FileInvalid)
        );
    }
}

#[test]
fn a_following_key_moves_to_the_new_default() {
    let ledger = ledger_of(&[("files.sort", "\"name\"")]);
    let before = "[files]\nsort = \"name\" # mine\n";
    let done = upgrade_text(before, &ledger, &sorted_by_date()).unwrap();
    assert_eq!(done.text, "[files]\nsort = \"date\" # mine\n");
    assert_eq!(done.written, pairs(&[("files.sort", "\"date\"")]));
    assert_eq!(
        done.changes,
        [DefaultChange {
            key: "files.sort".to_owned(),
            old: "\"name\"".to_owned(),
            new: "\"date\"".to_owned(),
        }]
    );
}

#[test]
fn a_key_the_user_changed_is_never_touched() {
    let ledger = ledger_of(&[("files.sort", "\"name\"")]);
    let before = "[files]\nsort = \"size\"\n";
    let done = upgrade_text(before, &ledger, &sorted_by_date()).unwrap();
    assert_eq!(done.text, before);
    assert_eq!(done.written, []);
    assert_eq!(done.changes, []);
}

#[test]
fn the_default_typed_by_hand_counts_as_following() {
    let ledger = ledger_of(&[("files.sort", "\"name\"")]);
    let before = "[files]\nsort = 'name'\n";
    let done = upgrade_text(before, &ledger, &sorted_by_date()).unwrap();
    assert_eq!(parse(&done.text).unwrap().0.files.sort, SortKey::Date);
    assert_eq!(done.changes.len(), 1);
}

#[test]
fn without_a_ledger_row_a_value_equal_to_the_default_is_recorded_as_following() {
    let before = "[files]\nsort = \"name\"\n";
    let done = upgrade_text(before, &Ledger::new(), &Settings::default()).unwrap();
    assert_eq!(done.text, before);
    assert_eq!(done.written, pairs(&[("files.sort", "\"name\"")]));
    assert_eq!(done.changes, []);
}

#[test]
fn without_a_ledger_row_an_old_default_typed_by_hand_is_the_users() {
    let before = "[files]\nsort = \"name\"\n";
    let done = upgrade_text(before, &Ledger::new(), &sorted_by_date()).unwrap();
    assert_eq!(done.text, before);
    assert_eq!(done.written, []);
    assert_eq!(done.changes, []);
}

#[test]
fn an_unchanged_default_rewrites_nothing() {
    let ledger = ledger_of(&DEFAULT_KEYS);
    let done = upgrade_text(FULL, &ledger, &Settings::default()).unwrap();
    assert_eq!(
        done,
        Reconciled {
            text: FULL.to_owned(),
            ..Reconciled::default()
        }
    );
}

#[test]
fn an_upgrade_keeps_crlf_and_bom() {
    let ledger = ledger_of(&[("files.sort", "\"name\"")]);
    let before = "\u{feff}# mine\r\n[files]\r\nsort = \"name\"\r\n";
    let done = upgrade_text(before, &ledger, &sorted_by_date()).unwrap();
    assert_eq!(
        done.text,
        "\u{feff}# mine\r\n[files]\r\nsort = \"date\"\r\n"
    );
}

#[test]
fn reconcile_of_an_empty_file_writes_and_records_every_default() {
    let done = reconcile("", &Ledger::new(), &Settings::default()).unwrap();
    assert_eq!(done.text, FULL);
    assert_eq!(done.written, pairs(&DEFAULT_KEYS));
}

#[test]
fn reconcile_updates_following_keys_and_adds_missing_ones_together() {
    let ledger = ledger_of(&DEFAULT_KEYS);
    let before = FULL.replace("font-size = 13\n", "");
    let done = reconcile(&before, &ledger, &sorted_by_date()).unwrap();
    let expected = FULL.replace("sort = \"name\"", "sort = \"date\"").replace(
        "font-size = 13\nui-scale = 100\ndensity = \"normal\"\nmodifier-hints = \"all\"\nmodifier-hints-delay-ms = 400\n",
        "ui-scale = 100\ndensity = \"normal\"\nmodifier-hints = \"all\"\nmodifier-hints-delay-ms = 400\nfont-size = 13\n",
    );
    assert_eq!(done.text, expected);
    assert_eq!(
        done.written,
        pairs(&[("files.sort", "\"date\""), ("appearance.font-size", "13")])
    );
    assert_eq!(done.changes.len(), 1);
}

#[test]
fn reconcile_of_a_complete_followed_file_is_byte_for_byte_the_same() {
    unchanged_by_reconcile(FULL);
    unchanged_by_reconcile(&FULL.replace('\n', "\r\n"));
    unchanged_by_reconcile(&format!("\u{feff}{}", FULL.replace('\n', "\r\n")));
    unchanged_by_reconcile(&format!(
        "# my settings\n\n{}\n\n# end\n",
        FULL.replace("mode = \"system\"", "mode=\"system\"   # comment")
    ));
}

#[test]
fn reconcile_twice_changes_nothing_the_second_time() {
    let first = reconcile(
        "[appearance]\nzoom = 2\n",
        &Ledger::new(),
        &sorted_by_date(),
    )
    .unwrap();
    let ledger: Ledger = first.written.iter().cloned().collect();
    let second = reconcile(&first.text, &ledger, &sorted_by_date()).unwrap();
    assert_eq!(second.text, first.text);
    assert_eq!(second.written, []);
    assert_eq!(second.changes, []);
}

#[test]
fn a_removed_key_gets_one_note_above_it() {
    let before = "[appearance]\nmode = \"dark\"\n# my zoom\nzoom = 2\n";
    let after = mark_unused(before).unwrap();
    assert_eq!(
        after,
        format!("[appearance]\nmode = \"dark\"\n# my zoom\n{UNUSED_NOTE}\nzoom = 2\n")
    );
    assert_eq!(mark_unused(&after).unwrap(), after);
}

#[test]
fn a_removed_section_gets_one_note_above_its_header() {
    let before = "[files]\nsort = \"size\"\n\n[plugins]\ngit = true\n";
    let after = mark_unused(before).unwrap();
    assert_eq!(
        after,
        format!("[files]\nsort = \"size\"\n\n{UNUSED_NOTE}\n[plugins]\ngit = true\n")
    );
    assert_eq!(mark_unused(&after).unwrap(), after);
}

#[test]
fn the_old_show_hidden_key_is_noted_as_unused() {
    let before = "[files]\nshow-hidden = false\n";
    assert_eq!(
        mark_unused(before).unwrap(),
        format!("[files]\n{UNUSED_NOTE}\nshow-hidden = false\n")
    );
}

#[test]
fn the_note_keeps_the_key_indent_and_line_endings() {
    let before = "[appearance]\r\n    zoom = 2\r\n";
    assert_eq!(
        mark_unused(before).unwrap(),
        format!("[appearance]\r\n    {UNUSED_NOTE}\r\n    zoom = 2\r\n")
    );
}

#[test]
fn known_keys_get_no_note() {
    assert_eq!(mark_unused(FULL).unwrap(), FULL);
}

#[test]
fn reconcile_notes_a_removed_key_once_across_runs() {
    let first = reconcile(
        "[appearance]\nzoom = 2\n",
        &Ledger::new(),
        &Settings::default(),
    )
    .unwrap();
    assert_eq!(first.text.matches(UNUSED_NOTE).count(), 1, "{}", first.text);
    let ledger: Ledger = first.written.iter().cloned().collect();
    let second = reconcile(&first.text, &ledger, &Settings::default()).unwrap();
    assert_eq!(second.text, first.text);
}

#[test]
fn reconcile_keeps_the_user_values_parsed_settings() {
    let before = "[appearance]\nfont-size = 16\n[files]\nsort = \"kind\"\n";
    let ledger = ledger_of(&DEFAULT_KEYS);
    let done = reconcile(before, &ledger, &sorted_by_date()).unwrap();
    let settings = parse(&done.text).unwrap().0;
    assert_eq!(settings.appearance.font_size, 16);
    assert_eq!(settings.files.sort, SortKey::Kind);
    assert_eq!(done.changes, []);
}

#[test]
fn a_file_of_only_comments_keeps_them_on_top() {
    for comment in ["# my settings\n", "# my settings"] {
        let done = reconcile(comment, &Ledger::new(), &Settings::default()).unwrap();
        assert_eq!(done.text, format!("# my settings\n{FULL}"));
    }
}

#[test]
fn a_removed_nested_section_is_noted_at_its_own_header() {
    let before = "[plugins.git]\non = true\n";
    assert_eq!(
        mark_unused(before).unwrap(),
        format!("{UNUSED_NOTE}\n[plugins.git]\non = true\n")
    );
}

fn header_above<'a>(text: &'a str, line: &str) -> &'a str {
    let at = text
        .find(line)
        .unwrap_or_else(|| panic!("{line:?} missing from {text}"));
    text[..at]
        .lines()
        .rev()
        .find(|l| l.starts_with('['))
        .unwrap_or_default()
}

fn line_after<'a>(text: &'a str, line: &str) -> Option<&'a str> {
    let at = text.find(line)? + line.len();
    text[at..].lines().nth(1)
}

#[test]
fn a_trailing_comment_stays_under_its_own_section() {
    for (before, comment) in [
        ("[files]\nsort = \"size\"\n# the end\n", "# the end"),
        ("[files]\nsort = \"size\"\n# the end", "# the end"),
        (
            "[files]\nsort = \"size\"\n# sort-descending = true\n",
            "# sort-descending = true",
        ),
        (
            "[files]\nsort = \"size\"\n\n# folders-first = false\n# old\n",
            "# folders-first = false\n# old",
        ),
    ] {
        let after = completed(before);
        assert!(after.starts_with("[files]\nsort = \"size\"\n"), "{after}");
        assert_eq!(header_above(&after, comment), "[files]", "{after}");
        let next = line_after(&after, comment);
        assert!(next.is_none_or(str::is_empty), "{after}");
        assert_eq!(after.matches(comment).count(), 1, "{after}");
        assert!(parse(&after).is_ok(), "{after}");
    }
}

#[test]
fn a_trailing_comment_under_the_last_section_stays_last_when_no_section_is_added() {
    let before = format!(
        "{}# the end\n",
        FULL.replace("sort-descending = false\n", "")
    );
    let after = completed(&before);
    assert!(after.contains("sort-descending = false\n"), "{after}");
    assert!(after.ends_with("who-uses = false\n# the end\n"), "{after}");
}

#[test]
fn a_mixed_file_keeps_each_line_ending_and_new_lines_take_the_first_one() {
    let before = "# mine\n[files]\r\nsort = \"size\"\r\n";
    let after = completed(before);
    assert!(after.starts_with(before), "{after:?}");
    let added = &after[before.len()..];
    assert!(!added.is_empty());
    assert!(!added.contains('\r'), "{after:?}");
    let crlf_first = "# mine\r\n[files]\nsort = \"size\"\n";
    let after = completed(crlf_first);
    assert!(after.starts_with(crlf_first), "{after:?}");
    let added = &after[crlf_first.len()..];
    assert_eq!(
        added.matches("\r\n").count(),
        added.matches('\n').count(),
        "{after:?}"
    );
}

#[test]
fn an_upgrade_in_a_mixed_file_keeps_the_line_ending_of_the_changed_line() {
    let ledger = ledger_of(&[("files.sort", "\"name\"")]);
    let before = "# mine\n[files]\r\nsort = \"name\"\r\n";
    let done = upgrade_text(before, &ledger, &sorted_by_date()).unwrap();
    assert_eq!(done.text, "# mine\n[files]\r\nsort = \"date\"\r\n");
}

fn without_viewers() -> String {
    let at = FULL.find("\n[text.view]").unwrap();
    FULL[..at].to_owned()
}

#[test]
fn only_the_viewer_sections_are_added_to_an_older_complete_file() {
    let after = completed(&without_viewers());
    assert_eq!(after, FULL);
    assert!(!after.contains("[text]\n"), "{after}");
    assert!(!after.contains("[image]\n"), "{after}");
}

#[test]
fn a_trailing_comment_stays_under_its_section_when_viewer_sections_are_added() {
    let before = format!("{}# the end\n", without_viewers());
    let after = completed(&before);
    assert_eq!(
        header_above(&after, "# the end"),
        "[window.view]",
        "{after}"
    );
    assert_eq!(after.matches("# the end").count(), 1, "{after}");
    assert!(parse(&after).is_ok(), "{after}");
}

fn without_window() -> String {
    let from = FULL.find("\n[window.view]").unwrap();
    let to = FULL.find("\n[text.view]").unwrap();
    format!("{}{}", &FULL[..from], &FULL[to..])
}

#[test]
fn only_the_window_section_is_added_to_an_older_complete_file() {
    let before = without_window();
    assert!(!before.contains("[window"), "{before}");
    let after = completed(&before);
    assert!(after.starts_with(&before), "{after}");
    assert!(
        after.ends_with(
            "marked-totals = true\ncontents = true\next-width = 0\nsize-width = 0\ndate-width = 0\n"
        ),
        "{after}"
    );
    assert_eq!(after.matches("[window.view]").count(), 1, "{after}");
    assert!(parse(&after).is_ok(), "{after}");
}

#[test]
fn a_hand_set_fkey_bar_stays_on_when_the_other_parts_are_added() {
    let before = format!(
        "{}\n[window.view]\nfkey-bar = true # bar on\n",
        without_window()
    );
    let after = completed(&before);
    assert!(after.contains("fkey-bar = true # bar on"), "{after}");
    assert!(after.contains("tabs-with-one = true"), "{after}");
    assert_eq!(after.matches("fkey-bar").count(), 1, "{after}");
}

fn without_automotive() -> String {
    let at = FULL.find("\n[automotive]").unwrap();
    FULL[..at].to_owned()
}

#[test]
fn only_the_automotive_section_is_added_to_an_older_complete_file() {
    let after = completed(&without_automotive());
    assert_eq!(after, FULL);
    assert_eq!(after.matches("[automotive]\n").count(), 1, "{after}");
}

#[test]
fn a_trailing_comment_survives_when_automotive_sections_are_added() {
    let before = format!("{}# the end\n", without_automotive());
    let after = completed(&before);
    assert_eq!(header_above(&after, "# the end"), "[search]", "{after}");
    assert_eq!(after.matches("# the end").count(), 1, "{after}");
    assert!(after.ends_with("who-uses = false\n"), "{after}");
    assert!(parse(&after).is_ok(), "{after}");
}
