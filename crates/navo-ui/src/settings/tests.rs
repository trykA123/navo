use super::*;
use crate::app_menu::Setting;
use crate::compare_visible::show_compare;
use crate::modifier_hints::HintsMode;
use crate::tokens::{Density, ThemeChoice};

fn spec(field: Field) -> &'static FieldSpec {
    field.spec()
}

#[test]
fn every_field_sits_at_its_own_index() {
    for (index, spec) in FIELDS.iter().enumerate() {
        assert_eq!(spec.field.index(), index, "{:?}", spec.field);
    }
}

#[test]
fn every_key_is_unique_and_lives_in_its_section() {
    for spec in &FIELDS {
        let home = match spec.section {
            Section::Display => spec.group().map(str::to_lowercase),
            Section::Safety => Some("retention".to_owned()),
            section => Some(section.label().to_lowercase()),
        };
        assert_eq!(Some(spec.table().to_owned()), home, "{}", spec.key);
        assert_eq!(
            FIELDS.iter().filter(|other| other.key == spec.key).count(),
            1
        );
    }
}

#[test]
fn sections_run_in_order_without_gaps() {
    let order: Vec<Section> = FIELDS.iter().map(|spec| spec.section).collect();
    let mut seen: Vec<Section> = Vec::new();
    for section in order {
        if seen.last() != Some(&section) {
            assert!(!seen.contains(&section), "{section:?} split");
            seen.push(section);
        }
    }
    assert_eq!(
        seen,
        Section::ALL
            .into_iter()
            .filter(|section| !matches!(section, Section::Diagnostics | Section::Help))
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_right_arrow_steps_a_choice_and_stops_at_the_end() {
    let sort = spec(Field::Sort);
    assert_eq!(
        nudged(sort, &Value::Pick(0), Nudge::Forward),
        Some(Value::Pick(1))
    );
    assert_eq!(nudged(sort, &Value::Pick(3), Nudge::Forward), None);
    assert_eq!(nudged(sort, &Value::Pick(0), Nudge::Back), None);
}

#[test]
fn space_cycles_a_choice() {
    let theme = spec(Field::Theme);
    assert_eq!(
        nudged(theme, &Value::Pick(2), Nudge::Flip),
        Some(Value::Pick(0))
    );
}

#[test]
fn arrows_set_a_toggle_and_space_flips_it() {
    let toggle = spec(Field::ShowSystem);
    assert_eq!(
        nudged(toggle, &Value::Flag(false), Nudge::Forward),
        Some(Value::Flag(true))
    );
    assert_eq!(nudged(toggle, &Value::Flag(true), Nudge::Forward), None);
    assert_eq!(
        nudged(toggle, &Value::Flag(true), Nudge::Back),
        Some(Value::Flag(false))
    );
    assert_eq!(
        nudged(toggle, &Value::Flag(true), Nudge::Flip),
        Some(Value::Flag(false))
    );
}

#[test]
fn arrows_step_a_number_and_text_needs_editing() {
    let size = spec(Field::FontSize);
    assert_eq!(
        nudged(size, &Value::Number(13), Nudge::Forward),
        Some(Value::Number(14))
    );
    assert_eq!(nudged(size, &Value::Number(1), Nudge::Back), None);
    let font = spec(Field::Font);
    assert_eq!(
        nudged(font, &Value::Text("Inter".into()), Nudge::Forward),
        None
    );
}

#[test]
fn tab_jumps_to_the_next_section_and_wraps() {
    assert_eq!(
        section_of(next_section(&Filter::default(), 0, true)),
        Section::Files
    );
    let last = first_row(Section::Help);
    assert_eq!(next_section(&Filter::default(), last, true), 0);
    assert_eq!(
        section_of(next_section(&Filter::default(), 0, false)),
        Section::Help
    );
    assert_eq!(
        section_of(next_section(&Filter::default(), FIELDS.len() - 1, true)),
        Section::Diagnostics
    );
    assert_eq!(
        section_of(next_section(
            &Filter::default(),
            first_row(Section::Diagnostics),
            true
        )),
        Section::Help
    );
}

#[test]
fn rows_move_across_sections_and_stop_at_the_ends() {
    let last_appearance = first_row(Section::Files) - 1;
    assert_eq!(
        section_of(moved(&Filter::default(), last_appearance, true)),
        Section::Files
    );
    assert_eq!(moved(&Filter::default(), 0, false), 0);
    assert_eq!(
        moved(&Filter::default(), FIELDS.len() - 1, true),
        FIELDS.len() - 1
    );
}

#[test]
fn defaults_show_hidden_files_and_follow_the_system() {
    let values = Values::default();
    assert_eq!(values.theme(), ThemeChoice::System);
    assert_eq!(values.density(), Density::Normal);
    assert!(values.show_hidden());
    assert_eq!(values.editor_path(), None);
}

#[test]
fn a_menu_tile_toggle_becomes_a_change_of_its_field() {
    let values = Values::default();
    let change = values.toggled(Setting::HiddenFiles);
    assert_eq!(change, Change::show_hidden(false));
    assert_eq!(change.key(), "files.hidden-files");
    assert!(!values.with(&change).show_hidden());
    let theme = values.toggled(Setting::Theme);
    assert_eq!(values.with(&theme).theme(), ThemeChoice::Light);
}

#[test]
fn appearance_has_the_modifier_hint_rows() {
    let hints = spec(Field::ModifierHints);
    assert_eq!(hints.section, Section::Appearance);
    assert_eq!(hints.key, "appearance.modifier-hints");
    let Control::Choices(options, Look::Segments) = hints.control else {
        panic!("modifier hints are not segments: {:?}", hints.control);
    };
    let toml: Vec<&str> = options.iter().map(|option| option.toml).collect();
    assert_eq!(toml, ["off", "fkeys", "all"]);
    let delay = spec(Field::ModifierHintsDelay);
    assert_eq!(delay.section, Section::Appearance);
    assert_eq!(delay.key, "appearance.modifier-hints-delay-ms");
    assert_eq!(delay.control, Control::Number);
    assert!(Field::ModifierHints.is_applied());
    assert!(Field::ModifierHintsDelay.is_applied());
}

#[test]
fn modifier_hints_default_to_every_shortcut_after_400_ms() {
    let values = Values::default();
    assert_eq!(values.modifier_hints(), HintsMode::All);
    assert_eq!(values.modifier_hints_delay_ms(), 400);
    let change = Change::modifier_hints(HintsMode::Fkeys);
    assert_eq!(change.key(), "appearance.modifier-hints");
    assert_eq!(values.with(&change).modifier_hints(), HintsMode::Fkeys);
}

#[test]
fn the_display_section_groups_the_window_and_each_viewer_in_order() {
    let keys: Vec<(Option<&str>, &str)> = FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Display)
        .map(|spec| (spec.group(), spec.key))
        .collect();
    let groups: Vec<Option<&str>> = keys.iter().map(|(group, _)| *group).collect();
    let expected = [
        ("Window", 15),
        ("Text", 3),
        ("Image", 1),
        ("Markdown", 1),
        ("Preview", 2),
        ("Automotive", 8),
    ];
    let expected: Vec<Option<&str>> = expected
        .iter()
        .flat_map(|(group, count)| std::iter::repeat_n(Some(*group), *count))
        .collect();
    assert_eq!(groups, expected);
    let keys: Vec<&str> = keys.into_iter().map(|(_, key)| key).collect();
    assert_eq!(keys[0], "window.view.tabs-with-one");
    assert_eq!(keys[11], "window.view.contents");
    assert_eq!(
        keys[12..],
        [
            "window.view.ext-width",
            "window.view.size-width",
            "window.view.date-width",
            "text.view.line-numbers",
            "text.view.line-ends",
            "text.view.syntax-theme",
            "image.view.details",
            "markdown.view.preview-split",
            "preview.view.type-bars",
            "preview.view.largest-files",
            "automotive.view.preset",
            "automotive.view.inspector",
            "automotive.view.memory-strip",
            "automotive.view.charts",
            "automotive.view.row-bars",
            "automotive.view.usage-bars",
            "automotive.view.who-uses",
            "automotive.modules",
        ]
    );
    assert!(!Field::AutomotiveCharts.is_applied());
    assert!(Field::AutomotiveInspector.is_applied());
}

#[test]
fn the_inspector_opens_by_itself_only_in_rich_or_when_custom_asks() {
    let custom = |inspector: bool| {
        Values::default()
            .with(&Change {
                field: Field::AutomotivePreset,
                value: Value::Pick(2),
            })
            .with(&Change {
                field: Field::AutomotiveInspector,
                value: Value::Flag(inspector),
            })
    };
    let rich = Values::default().with(&Change {
        field: Field::AutomotivePreset,
        value: Value::Pick(1),
    });
    let simple_with_flag = Values::default().with(&Change {
        field: Field::AutomotiveInspector,
        value: Value::Flag(true),
    });
    assert!(!Values::default().opens_inspector());
    assert!(!simple_with_flag.opens_inspector());
    assert!(rich.opens_inspector());
    assert!(custom(true).opens_inspector());
    assert!(!custom(false).opens_inspector());
}

#[test]
fn every_display_row_has_a_one_sentence_hint() {
    for spec in FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Display)
    {
        let help = spec.help();
        assert!(help.ends_with('.'), "{}", spec.key);
        assert_eq!(help.matches(". ").count(), 0, "{}", spec.key);
    }
}

#[test]
fn text_parts_default_to_line_numbers_on_and_line_ends_off() {
    let values = Values::default();
    assert!(values.flag(Field::TextLineNumbers));
    assert!(!values.flag(Field::TextLineEnds));
    assert!(!values.flag(Field::ImageDetails));
    assert!(values.is_shown(None, Field::TextLineNumbers));
}

#[test]
fn the_markdown_split_and_both_preview_parts_default_to_on() {
    let values = Values::default();
    for field in [
        Field::MarkdownPreviewSplit,
        Field::PreviewTypeBars,
        Field::PreviewLargestFiles,
    ] {
        assert!(values.flag(field), "{}", field.spec().key);
        assert!(!values.is_changed(field));
        assert!(field.is_applied());
    }
    let hidden = values.with(&Change {
        field: Field::PreviewTypeBars,
        value: Value::Flag(false),
    });
    assert!(hidden.is_changed(Field::PreviewTypeBars));
    assert!(hidden.flag(Field::PreviewLargestFiles));
}

#[test]
fn every_window_part_but_the_fkey_bar_is_on_by_default_and_reads_back() {
    let values = Values::default();
    assert_eq!(WindowParts::of(&values), WindowParts::default());
    assert!(!WindowParts::default().fkey_bar);
    assert_eq!(
        WindowParts {
            fkey_bar: true,
            ..WindowParts::default()
        },
        WindowParts::all(true)
    );
    let hidden = values.with(&Change {
        field: Field::WindowSizeColumn,
        value: Value::Flag(false),
    });
    assert!(!WindowParts::of(&hidden).size_column);
    assert!(WindowParts::of(&hidden).date_column);
    assert!(hidden.is_changed(Field::WindowSizeColumn));
    assert!(Field::WindowContents.is_applied());
}

#[test]
fn only_the_twelve_window_fields_are_window_parts() {
    let parts: Vec<Field> = FIELDS
        .iter()
        .map(|spec| spec.field)
        .filter(|field| field.is_window_part())
        .collect();
    assert_eq!(parts.len(), 12);
    assert!(
        FIELDS
            .iter()
            .filter(|spec| spec.key.starts_with("window.") && spec.control == Control::Toggle)
            .all(|spec| spec.field.is_window_part())
    );
    assert!(!Field::WindowDateWidth.is_window_part());
}

#[test]
fn the_scroll_child_of_a_row_counts_the_group_headings_above_it() {
    let child = |field: Field| {
        child_of(
            &Filter::default(),
            FIELDS.iter().position(|spec| spec.field == field).unwrap(),
        )
    };
    assert_eq!(child(Field::Theme), Some(1));
    assert_eq!(child(Field::WindowTabsWithOne), Some(2));
    assert_eq!(child(Field::WindowContents), Some(13));
    assert_eq!(child(Field::TextLineNumbers), Some(18));
    assert_eq!(child(Field::ImageDetails), Some(22));
}

#[test]
fn the_copy_mode_row_sits_in_its_own_section_with_plain_help() {
    let row = spec(Field::CopyMode);
    assert_eq!(row.section, Section::Copy);
    assert_eq!(row.key, "copy.mode");
    assert_eq!(
        row.help,
        "Auto copies several files at once where the drives allow; Standard copies one file at a time."
    );
    assert!(matches!(row.control, Control::Choices(choices, Look::Segments) if choices.len() == 2));
    assert!(Field::CopyMode.is_applied());
}

#[test]
fn the_copy_mode_starts_auto_and_follows_the_pick() {
    let values = Values::default();
    assert_eq!(values.copy_mode(), navo_fs::CopyMode::Auto);
    assert!(!values.is_changed(Field::CopyMode));
    let standard = values.with(&Change {
        field: Field::CopyMode,
        value: Value::Pick(1),
    });
    assert_eq!(standard.copy_mode(), navo_fs::CopyMode::Standard);
    assert!(standard.is_changed(Field::CopyMode));
}

#[test]
fn files_has_the_drop_confirm_row_defaulting_to_never() {
    let row = spec(Field::DropConfirm);
    assert_eq!(row.section, Section::Files);
    assert_eq!(row.key, "files.drop-confirm");
    let Control::Choices(options, Look::Segments) = row.control else {
        panic!("drop confirm is not segments: {:?}", row.control);
    };
    let toml: Vec<&str> = options.iter().map(|option| option.toml).collect();
    assert_eq!(toml, ["never", "moves", "always"]);
    assert_eq!(Values::default().get(Field::DropConfirm), &Value::Pick(0));
}

#[test]
fn files_has_the_size_mode_and_measure_rows() {
    let row = spec(Field::SizeMode);
    assert_eq!(row.section, Section::Files);
    assert_eq!(row.key, "files.size-mode");
    let Control::Choices(options, Look::Segments) = row.control else {
        panic!("size mode is not segments: {:?}", row.control);
    };
    let toml: Vec<&str> = options.iter().map(|option| option.toml).collect();
    assert_eq!(toml, ["apparent", "on-disk"]);
    assert_eq!(
        Values::default().size_mode(),
        crate::browser::SizeMode::Apparent
    );
    let on_disk = Values::default().with(&Change {
        field: Field::SizeMode,
        value: Value::Pick(1),
    });
    assert_eq!(on_disk.size_mode(), crate::browser::SizeMode::OnDisk);
    let row = spec(Field::MeasureShownFolders);
    assert_eq!(row.key, "files.measure-shown-folders");
    assert!(matches!(row.control, Control::Toggle));
    assert!(!Values::default().flag(Field::MeasureShownFolders));
}

#[test]
fn the_terminal_provider_defaults_to_the_system_default_listed_first() {
    let row = spec(Field::TerminalProvider);
    let Control::Choices(options, Look::Segments) = row.control else {
        panic!("terminal provider is not segments: {:?}", row.control);
    };
    let toml: Vec<&str> = options.iter().map(|option| option.toml).collect();
    assert_eq!(
        toml,
        ["system", "windows-terminal", "powershell", "cmd", "custom"]
    );
    assert_eq!(options[0].text(), "System default");
    let values = Values::default();
    assert_eq!(values.get(Field::TerminalProvider), &Value::Pick(0));
    assert_eq!(values.terminal_provider(), "system");
    let explicit = values.with(&Change {
        field: Field::TerminalProvider,
        value: Value::Pick(1),
    });
    assert_eq!(explicit.terminal_provider(), "windows-terminal");
}

#[test]
fn the_search_page_has_one_row_per_search_key_with_the_store_defaults() {
    let keys: Vec<&str> = rows_in(Section::Search).map(|(_, spec)| spec.key).collect();
    assert_eq!(
        keys,
        [
            "search.index",
            "search.drives",
            "search.exclude",
            "search.network",
            "search.removable",
            "search.remember",
        ]
    );
    let values = Values::default();
    assert_eq!(values.get(Field::SearchIndex), &Value::Pick(0));
    assert_eq!(values.get(Field::SearchRemember), &Value::Flag(true));
    assert_eq!(values.get(Field::SearchNetwork), &Value::Flag(false));
    assert_eq!(
        values.get(Field::SearchExclude),
        &Value::Text("$Recycle.Bin, System Volume Information".into())
    );
    assert!(Field::SearchDrives.is_list() && Field::SearchExclude.is_list());
    assert!(rows_in(Section::Search).all(|(_, spec)| spec.field.is_applied()));
}

#[test]
fn drives_and_removable_rows_show_only_on_windows() {
    let shown = |field: Field| Filter::default().admits(spec(field));
    assert_eq!(shown(Field::SearchDrives), cfg!(windows));
    assert_eq!(shown(Field::SearchRemovable), cfg!(windows));
    assert!(shown(Field::SearchIndex) && shown(Field::SearchExclude));
}

#[test]
fn the_search_page_ends_with_the_service_and_history_blocks() {
    let found = lines(&Filter::default(), Section::Search);
    assert_eq!(&found[found.len() - 2..], [Line::Service, Line::History]);
    let only_history = lines(&Filter::new("clear"), Section::Search);
    assert_eq!(
        only_history,
        [Line::Section(Section::Search), Line::History]
    );
}

#[test]
fn ui_scale_is_next_to_font_size_and_steps_by_ten() {
    assert_eq!(Field::UiScale.index(), Field::FontSize.index() + 1);
    assert_eq!(Values::default().get(Field::UiScale), &Value::Number(100));
    let scale = spec(Field::UiScale);
    assert_eq!(
        nudged(scale, &Value::Number(100), Nudge::Forward),
        Some(Value::Number(110))
    );
    assert_eq!(
        nudged(scale, &Value::Number(100), Nudge::Back),
        Some(Value::Number(90))
    );
}

#[test]
fn the_safety_net_section_lists_its_five_retention_fields() {
    let keys: Vec<&str> = FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Safety)
        .map(|spec| spec.key)
        .collect();
    assert_eq!(
        keys,
        [
            "retention.retain-deletes",
            "retention.retain-overwrites",
            "retention.days",
            "retention.max-volume-mib",
            "retention.min-free-mib",
        ]
    );
    assert_eq!(Section::Safety.label(), "settings-safety");
    assert!(spec(Field::RetentionDays).help().contains("Not a backup"));
}

#[test]
fn the_theme_help_names_the_platform() {
    let help = spec(Field::Theme).help();
    assert_eq!(
        help.contains("Windows"),
        cfg!(windows),
        "theme help was: {help}"
    );
}

#[test]
fn system_files_are_a_windows_only_setting() {
    let shown = Filter::new("").admits(spec(Field::ShowSystem));
    assert_eq!(shown, cfg!(windows));
}

#[test]
fn the_sort_choice_for_extensions_is_not_called_kind() {
    assert!(SORTS.iter().all(|choice| choice.text() != "Kind"));
    assert_eq!(SORTS[3].text(), "Extension");
}

#[test]
fn the_tray_defaults_are_off_and_win_alt_n() {
    let values = Values::default();
    assert!(!values.keeps_in_tray());
    assert_eq!(values.summon_key(), "Win+Alt+N");
}

#[test]
fn the_tray_settings_are_windows_only() {
    for field in [Field::KeepInTray, Field::SummonKey] {
        assert_eq!(Filter::new("").admits(spec(field)), cfg!(windows));
    }
    assert_eq!(
        Filter::default().first_in(Section::Tray).is_some(),
        cfg!(windows)
    );
}

#[test]
fn the_git_section_follows_search_with_seven_rows() {
    let at = |section| Section::ALL.iter().position(|s| *s == section);
    assert_eq!(at(Section::Git), at(Section::Search).map(|i| i + 1));
    let keys: Vec<&str> = rows_in(Section::Git).map(|(_, spec)| spec.key).collect();
    assert_eq!(
        keys,
        [
            "git.badges",
            "git.tool",
            "git.tool-command",
            "git.engine",
            "git.fetch",
            "git.fetch-minutes",
            "git.network"
        ]
    );
}

#[test]
fn the_git_defaults_match_the_store() {
    let values = Values::default();
    assert!(values.git_badges() && !values.git_fetch() && !values.git_network());
    assert_eq!(values.git_engine(), "auto");
    assert_eq!(values.git_fetch_minutes(), 15);
    assert_eq!(values.changed_count(), 0);
}

#[test]
fn the_fetch_minutes_show_clamped_and_step_inside_the_range() {
    let set = |minutes| {
        Values::default().with(&Change {
            field: Field::GitFetchMinutes,
            value: Value::Number(minutes),
        })
    };
    assert_eq!(set(1).git_fetch_minutes(), 5);
    assert_eq!(set(500).git_fetch_minutes(), 120);
    let minutes = spec(Field::GitFetchMinutes);
    assert_eq!(nudged(minutes, &Value::Number(5), Nudge::Back), None);
    assert_eq!(nudged(minutes, &Value::Number(120), Nudge::Forward), None);
    assert_eq!(
        nudged(minutes, &Value::Number(15), Nudge::Forward),
        Some(Value::Number(16))
    );
}

#[test]
fn the_minutes_row_is_disabled_while_fetch_is_off() {
    let off = Values::default();
    assert!(!off.is_enabled(Field::GitFetchMinutes));
    assert!(off.is_enabled(Field::GitNetwork));
    let on = off.with(&Change {
        field: Field::GitFetch,
        value: Value::Flag(true),
    });
    assert!(on.is_enabled(Field::GitFetchMinutes));
}

#[test]
fn a_git_choice_round_trips_to_its_toml_name() {
    let pick = |name: &str| {
        let Control::Choices(options, _) = spec(Field::GitEngine).control else {
            return Value::Unset;
        };
        options
            .iter()
            .position(|option| option.toml == name)
            .map_or(Value::Unset, Value::Pick)
    };
    for name in ["auto", "gix", "git"] {
        let values = Values::default().with(&Change {
            field: Field::GitEngine,
            value: pick(name),
        });
        assert_eq!(values.git_engine(), name);
    }
}

#[test]
fn git_rows_are_found_by_their_words() {
    for (words, field) in [
        ("git status", Field::GitBadges),
        ("gix", Field::GitEngine),
        ("git program menu", Field::GitTool),
        ("command other path", Field::GitToolCommand),
        ("remote-tracking", Field::GitFetch),
        ("every minutes", Field::GitFetchMinutes),
        ("network drives git", Field::GitNetwork),
    ] {
        assert!(Filter::new(words).admits(spec(field)), "{words}");
    }
    assert!(Filter::new("git").first_in(Section::Git).is_some());
}

fn tool_row(id: &str, label: &str, path: &str, selected: bool) -> crate::git::ToolRow {
    crate::git::ToolRow {
        id: id.to_owned(),
        label: label.to_owned(),
        path: path.to_owned(),
        selected,
    }
}

fn found_tools() -> GitPage {
    GitPage::new(
        vec![
            tool_row("tortoisegit", "TortoiseGit", r"C:\Git\Tortoise.exe", true),
            tool_row("git-cli", "Git command line", r"C:\Git\git.exe", false),
            tool_row("custom", "Custom command", "", false),
        ],
        None,
    )
}

#[test]
fn the_git_tool_defaults_to_auto_and_hides_the_command() {
    let values = Values::default();
    assert_eq!(values.git_tool(), "auto");
    assert_eq!(values.hidden_row(), Some(Field::GitToolCommand));
    assert!(
        !Filter::default()
            .hiding(values.hidden_row())
            .admits(spec(Field::GitToolCommand))
    );
    let other = values.with(&Change::text(Field::GitTool, "custom"));
    assert_eq!(other.hidden_row(), None);
    assert!(
        Filter::default()
            .hiding(other.hidden_row())
            .admits(spec(Field::GitToolCommand))
    );
}

#[test]
fn a_hidden_row_is_skipped_by_the_arrow_keys_and_the_list() {
    let filter = Filter::default().hiding(Some(Field::GitToolCommand));
    let tool = Field::GitTool.index();
    assert_eq!(moved(&filter, tool, true), Field::GitEngine.index());
    assert!(!lines(&filter, Section::Git).contains(&Line::Row(Field::GitToolCommand.index())));
}

#[test]
fn auto_names_the_tool_it_resolved_to_only_while_chosen() {
    let page = found_tools();
    assert!(page.options("auto")[0].label.contains("TortoiseGit"));
    assert!(!page.options("fork")[0].label.contains("TortoiseGit"));
}

#[test]
fn the_tool_options_carry_each_path_and_the_fixed_choices() {
    let options = found_tools().options("auto");
    let ids: Vec<&str> = options.iter().map(|option| option.id.as_str()).collect();
    assert_eq!(ids, ["auto", "tortoisegit", "git-cli", "custom", "none"]);
    assert_eq!(options[1].path, r"C:\Git\Tortoise.exe");
    assert_eq!(options[2].path, r"C:\Git\git.exe");
}

#[test]
fn a_stored_tool_that_is_not_installed_still_has_its_option() {
    let options = found_tools().options("fork");
    assert_eq!(options[1].id, "fork");
}

#[test]
fn the_arrow_keys_walk_the_tool_options() {
    let page = found_tools();
    let text = |id: &str| Some(Value::Text(id.to_owned().into()));
    assert_eq!(page.nudged("auto", Nudge::Forward), text("tortoisegit"));
    assert_eq!(page.nudged("auto", Nudge::Back), None);
    assert_eq!(page.nudged("none", Nudge::Forward), None);
    assert_eq!(page.nudged("none", Nudge::Flip), text("auto"));
}

#[test]
fn the_clipboard_defaults_are_on_and_win_alt_v() {
    let values = Values::default();
    assert!(values.clipboard_on());
    assert!(values.records_clipboard());
    assert_eq!(values.clipboard_key(), "Win+Alt+V");
}

#[test]
fn only_the_clipboard_switch_shows_off_windows() {
    let shown = |field| Filter::new("").admits(spec(field));
    assert!(shown(Field::ClipboardOn));
    assert_eq!(shown(Field::ClipboardRecord), cfg!(windows));
    assert_eq!(shown(Field::ClipboardKey), cfg!(windows));
    assert!(Filter::default().first_in(Section::Clipboard).is_some());
}

#[test]
fn the_compare_section_lists_its_five_fields_in_order() {
    let keys: Vec<&str> = FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Compare)
        .map(|spec| spec.key)
        .collect();
    assert_eq!(
        keys,
        [
            "compare.git-engine",
            "compare.ignore-list",
            "compare.respect-gitignore",
            "compare.memory-budget-mib",
            "compare.content-check",
        ]
    );
    assert_eq!(Section::Compare.label(), "Compare");
    let _shown = show_compare();
    assert!(Filter::default().first_in(Section::Compare).is_some());
}

#[test]
fn the_compare_defaults_match_the_store() {
    let values = Values::default();
    assert_eq!(values.get(Field::GitEngine), &Value::Pick(0));
    assert_eq!(
        values.get(Field::CompareIgnoreList),
        &Value::Text("".into())
    );
    assert!(values.flag(Field::CompareRespectGitignore));
    assert_eq!(values.get(Field::CompareBudgetMib), &Value::Number(1024));
    assert_eq!(values.get(Field::CompareContentCheck), &Value::Pick(0));
}

#[test]
fn the_compare_choices_use_the_stored_names() {
    let names = |field| match spec(field).control {
        Control::Choices(options, _) => options.iter().map(|option| option.toml).collect(),
        _ => Vec::new(),
    };
    assert_eq!(names(Field::CompareGitEngine), ["git", "gix", "auto"]);
    assert_eq!(names(Field::CompareContentCheck), ["content", "size-time"]);
}

#[test]
fn the_compare_rows_have_texts_and_the_ignore_list_is_a_list() {
    for spec in FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Compare)
    {
        assert!(!spec.label().starts_with("settings-"), "{}", spec.key);
        assert!(!spec.help().starts_with("settings-"), "{}", spec.key);
    }
    assert!(Field::CompareIgnoreList.is_list());
    assert!(!Field::GitEngine.is_list());
}

#[test]
fn the_search_finds_the_compare_rows_on_every_platform() {
    let _shown = show_compare();
    let found = |query: &str, field| Filter::new(query).admits(spec(field));
    assert!(found("git revisions", Field::CompareGitEngine));
    assert!(found("gitignore", Field::CompareRespectGitignore));
    assert!(found("memory", Field::CompareBudgetMib));
    assert!(found("ignore", Field::CompareIgnoreList));
    assert!(found("content", Field::CompareContentCheck));
}

#[test]
fn a_hidden_compare_section_has_no_rows_hits_or_lines() {
    let hit = |query: &str| Filter::new(query).first_row().map(section_of);
    assert_eq!(Filter::default().first_in(Section::Compare), None);
    assert_eq!(hit("git revisions"), None);
    assert_eq!(hit("memory budget"), None);
    let shown: Vec<Line> = lines(&Filter::new("compare"), Section::Appearance);
    assert!(!shown.contains(&Line::Section(Section::Compare)));
    let walked = next_section(&Filter::default(), first_row(Section::Clipboard), true);
    assert_eq!(section_of(walked), Section::Diagnostics);
}

#[test]
fn a_visible_compare_section_has_rows_hits_and_a_stop_in_the_walk() {
    let _shown = show_compare();
    let hit = Filter::new("git revisions").first_row().map(section_of);
    assert_eq!(hit, Some(Section::Compare));
    let walked = next_section(&Filter::default(), first_row(Section::Clipboard), true);
    assert_eq!(section_of(walked), Section::Compare);
    assert!(lines(&Filter::default(), Section::Compare).contains(&Line::Section(Section::Compare)));
}
