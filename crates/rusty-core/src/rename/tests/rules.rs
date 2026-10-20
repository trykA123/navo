use crate::rename::{
    Compiled, Counter, ExtCase, Find, LocalTime, NameCase, RenameRules, RuleField, Subject,
    split_name,
};

fn rename_at(rules: &RenameRules, name: &str, index: u64) -> String {
    let compiled = Compiled::new(rules).unwrap_or_else(|e| panic!("rules should compile: {e}"));
    let (stem, ext) = split_name(name);
    let subject = Subject {
        stem,
        ext,
        parent: "Photos",
        grand: "Trips",
        time: Some(LocalTime {
            year: 2026,
            month: 3,
            day: 7,
            hour: 9,
            minute: 5,
            second: 1,
        }),
        fields: None,
    };
    compiled.new_name(&subject, index)
}

fn rename(rules: &RenameRules, name: &str) -> String {
    rename_at(rules, name, 0)
}

fn find(text: &str, replace: &str) -> RenameRules {
    RenameRules {
        find: Find {
            text: text.to_owned(),
            replace: replace.to_owned(),
            ..Find::default()
        },
        ..RenameRules::default()
    }
}

fn with_find(rules: RenameRules, edit: impl FnOnce(&mut Find)) -> RenameRules {
    let mut rules = rules;
    edit(&mut rules.find);
    rules
}

#[test]
fn default_rules_keep_the_name() {
    let rules = RenameRules::default();
    assert_eq!(rename(&rules, "Report.final.TXT"), "Report.final.TXT");
    assert_eq!(rename(&rules, ".gitignore"), ".gitignore");
    assert_eq!(rename(&rules, "README"), "README");
}

#[test]
fn masks_run_before_search_and_case() {
    let rules = RenameRules {
        name_mask: "[N]-copy".to_owned(),
        ext_mask: "bak".to_owned(),
        name_case: NameCase::Upper,
        find: Find {
            text: "copy".to_owned(),
            replace: "dup".to_owned(),
            ..Find::default()
        },
        ..RenameRules::default()
    };
    assert_eq!(rename(&rules, "notes.txt"), "NOTES-DUP.bak");
}

#[test]
fn case_runs_after_replace() {
    let mut rules = find("a", "b");
    rules.name_case = NameCase::Upper;
    assert_eq!(rename(&rules, "aaa.txt"), "BBB.txt");
}

#[test]
fn an_empty_extension_adds_no_dot() {
    let rules = RenameRules {
        name_mask: "[N]x".to_owned(),
        ..RenameRules::default()
    };
    assert_eq!(rename(&rules, "README"), "READMEx");
    let rules = RenameRules {
        ext_mask: String::new(),
        ..RenameRules::default()
    };
    assert_eq!(rename(&rules, "a.txt"), "a");
}

#[test]
fn plain_search_replaces_every_match_and_ignores_case() {
    assert_eq!(rename(&find("ab", "-"), "AbxaBxab.txt"), "-x-x-.txt");
}

#[test]
fn plain_search_treats_regex_characters_and_dollars_literally() {
    assert_eq!(rename(&find("a.b", "$1"), "a.b axb.txt"), "$1 axb.txt");
    assert_eq!(rename(&find("(x)", "[y]"), "a(x)b.txt"), "a[y]b.txt");
}

#[test]
fn match_case_makes_plain_search_exact() {
    let rules = with_find(find("ab", "-"), |f| f.match_case = true);
    assert_eq!(rename(&rules, "Abab.txt"), "Ab-.txt");
}

#[test]
fn pipe_separates_pairs_applied_in_order() {
    assert_eq!(rename(&find("a|b", "x|y"), "aabb.txt"), "xxyy.txt");
    assert_eq!(rename(&find("a|b", "b|c"), "ab.txt"), "cc.txt");
}

#[test]
fn one_replacement_applies_to_every_pair() {
    assert_eq!(rename(&find("a|b", "-"), "abc.txt"), "--c.txt");
}

#[test]
fn a_missing_replacement_deletes_the_match() {
    assert_eq!(rename(&find("a|b|c", "x|y"), "abc.txt"), "xy.txt");
    assert_eq!(rename(&find("a|b", "x|"), "abc.txt"), "xc.txt");
}

#[test]
fn empty_find_parts_are_skipped() {
    assert_eq!(rename(&find("|a|", "x"), "aa.txt"), "xx.txt");
    assert_eq!(rename(&find("", "x"), "aa.txt"), "aa.txt");
}

#[test]
fn first_only_replaces_one_match_per_pair() {
    let rules = with_find(find("a", "x"), |f| f.first_only = true);
    assert_eq!(rename(&rules, "aaa.txt"), "xaa.txt");
    let rules = with_find(find("a|b", "x|y"), |f| f.first_only = true);
    assert_eq!(rename(&rules, "aabb.txt"), "xayb.txt");
}

#[test]
fn search_covers_the_extension_unless_name_only() {
    assert_eq!(rename(&find("txt", "md"), "txt.txt"), "md.md");
    let rules = with_find(find("txt", "md"), |f| f.name_only = true);
    assert_eq!(rename(&rules, "txt.txt"), "md.txt");
}

#[test]
fn search_can_change_the_extension() {
    assert_eq!(rename(&find(".jpeg", ".jpg"), "photo.jpeg"), "photo.jpg");
}

#[test]
fn regex_groups_expand_in_the_replacement() {
    let rules = with_find(find(r"(\w+)_(\d+)", "$2-$1"), |f| f.regex = true);
    assert_eq!(rename(&rules, "scan_042.png"), "042-scan.png");
}

#[test]
fn regex_named_groups_expand() {
    let rules = with_find(
        find(r"(?P<word>[a-z]+)(?P<num>\d+)", "${num}${word}"),
        |f| {
            f.regex = true;
        },
    );
    assert_eq!(rename(&rules, "ab12.txt"), "12ab.txt");
}

#[test]
fn regex_alternation_is_not_a_pair_separator() {
    let rules = with_find(find("a|b", "x"), |f| f.regex = true);
    assert_eq!(rename(&rules, "abc.txt"), "xxc.txt");
}

#[test]
fn regex_honours_match_case_and_first_only() {
    let rules = with_find(find("[a-z]", "_"), |f| f.regex = true);
    assert_eq!(rename(&rules, "aBc"), "___");
    let rules = with_find(find("[a-z]", "_"), |f| {
        f.regex = true;
        f.match_case = true;
    });
    assert_eq!(rename(&rules, "aBc"), "_B_");
    let rules = with_find(find("[a-z]", "_"), |f| {
        f.regex = true;
        f.match_case = true;
        f.first_only = true;
    });
    assert_eq!(rename(&rules, "aBc"), "_Bc");
}

#[test]
fn regex_name_only_leaves_the_extension() {
    let rules = with_find(find(r"\w+$", "x"), |f| {
        f.regex = true;
        f.name_only = true;
    });
    assert_eq!(rename(&rules, "abc.txt"), "x.txt");
}

#[test]
fn plain_search_matches_non_ascii_case_insensitively() {
    assert_eq!(rename(&find("ÄÖ", "ae"), "Fäöx.txt"), "Faex.txt");
}

#[test]
fn name_case_modes_handle_non_ascii_names() {
    let case = |mode, name: &str| {
        let rules = RenameRules {
            name_case: mode,
            ..RenameRules::default()
        };
        rename(&rules, name)
    };
    assert_eq!(case(NameCase::Unchanged, "ÉcoLE.txt"), "ÉcoLE.txt");
    assert_eq!(case(NameCase::Lower, "ÉCOLE Ärger.txt"), "école ärger.txt");
    assert_eq!(
        case(NameCase::Upper, "école straße.txt"),
        "ÉCOLE STRASSE.txt"
    );
    assert_eq!(
        case(NameCase::FirstLetter, "éCOLE ÄRGER.txt"),
        "École ärger.txt"
    );
    assert_eq!(
        case(NameCase::Title, "éCOLE ärger_über-alles.x.txt"),
        "École Ärger_Über-Alles.X.txt"
    );
}

#[test]
fn lower_case_uses_the_final_sigma() {
    let rules = RenameRules {
        name_case: NameCase::Lower,
        ..RenameRules::default()
    };
    assert_eq!(rename(&rules, "ΣΑΣ.txt"), "σας.txt");
}

#[test]
fn extension_case_modes_leave_the_name_alone() {
    let case = |mode, name: &str| {
        let rules = RenameRules {
            ext_case: mode,
            ..RenameRules::default()
        };
        rename(&rules, name)
    };
    assert_eq!(case(ExtCase::Unchanged, "Ab.JpG"), "Ab.JpG");
    assert_eq!(case(ExtCase::Lower, "Ab.JpG"), "Ab.jpg");
    assert_eq!(case(ExtCase::Upper, "Ab.jpé"), "Ab.JPÉ");
    assert_eq!(case(ExtCase::Upper, "Ab"), "Ab");
}

#[test]
fn bare_counter_uses_the_form_start_step_and_digits() {
    let rules = RenameRules {
        name_mask: "img[C]".to_owned(),
        counter: Counter {
            start: 5,
            step: 3,
            digits: 3,
        },
        ..RenameRules::default()
    };
    assert_eq!(rename_at(&rules, "a.jpg", 0), "img005.jpg");
    assert_eq!(rename_at(&rules, "a.jpg", 1), "img008.jpg");
    assert_eq!(rename_at(&rules, "a.jpg", 4), "img017.jpg");
}

#[test]
fn counter_padding_does_not_cut_wide_numbers() {
    let rules = RenameRules {
        name_mask: "[C]".to_owned(),
        counter: Counter {
            start: 98,
            step: 1,
            digits: 2,
        },
        ..RenameRules::default()
    };
    assert_eq!(rename_at(&rules, "a.jpg", 2), "100.jpg");
}

#[test]
fn partial_counters_take_missing_parts_from_the_form() {
    let counter = Counter {
        start: 7,
        step: 2,
        digits: 4,
    };
    let render = |mask: &str, index| {
        let rules = RenameRules {
            name_mask: mask.to_owned(),
            counter,
            ..RenameRules::default()
        };
        rename_at(&rules, "a.jpg", index)
    };
    assert_eq!(render("[C10]", 1), "0012.jpg");
    assert_eq!(render("[C+5]", 1), "0012.jpg");
    assert_eq!(render("[C:3]", 1), "009.jpg");
    assert_eq!(render("[C10:3]", 1), "012.jpg");
    assert_eq!(render("[C1+1:1]", 1), "2.jpg");
}

#[test]
fn counter_value_can_be_searched_and_cased() {
    let mut rules = find("0", "o");
    rules.name_mask = "x[C:2]".to_owned();
    rules.name_case = NameCase::Upper;
    assert_eq!(rename(&rules, "a.jpg"), "XO1.jpg");
}

#[test]
fn counter_rules_outside_their_range_are_errors() {
    for (counter, label) in [
        (
            Counter {
                start: 1,
                step: 0,
                digits: 1,
            },
            "step 0",
        ),
        (
            Counter {
                start: 1,
                step: 1,
                digits: 0,
            },
            "digits 0",
        ),
        (
            Counter {
                start: 1,
                step: 1,
                digits: 11,
            },
            "digits 11",
        ),
    ] {
        let rules = RenameRules {
            counter,
            ..RenameRules::default()
        };
        let err = Compiled::new(&rules).err().map(|e| e.field);
        assert_eq!(err, Some(RuleField::Counter), "{label}");
    }
}

#[test]
fn invalid_regex_names_the_find_field() {
    let rules = with_find(find("(unclosed", "x"), |f| f.regex = true);
    let err = Compiled::new(&rules)
        .err()
        .expect("regex should be refused");
    assert_eq!(err.field, RuleField::Find);
    assert!(!err.message.is_empty());
}

#[test]
fn plain_mode_never_fails_on_regex_characters() {
    assert!(Compiled::new(&find("(unclosed", "x")).is_ok());
}

#[test]
fn bad_masks_name_their_field() {
    let name = RenameRules {
        name_mask: "[Q]".to_owned(),
        ..RenameRules::default()
    };
    let ext = RenameRules {
        ext_mask: "[N2--]".to_owned(),
        ..RenameRules::default()
    };
    assert_eq!(
        Compiled::new(&name).err().map(|e| e.field),
        Some(RuleField::NameMask)
    );
    let err = Compiled::new(&ext).err().expect("mask should be refused");
    assert_eq!(err.field, RuleField::ExtMask);
    assert!(err.message.contains("character 1"), "{}", err.message);
}

#[test]
fn rules_survive_a_json_round_trip() {
    let rules = RenameRules {
        name_mask: "[N]_[C10+5:3]".to_owned(),
        ext_mask: "[E]".to_owned(),
        find: Find {
            text: "a|b".to_owned(),
            replace: "x|y".to_owned(),
            regex: true,
            match_case: true,
            first_only: true,
            name_only: true,
        },
        name_case: NameCase::Title,
        ext_case: ExtCase::Lower,
        counter: Counter {
            start: 4,
            step: 2,
            digits: 3,
        },
        ..RenameRules::default()
    };
    let json = serde_json::to_string(&rules).expect("rules should serialise");
    let back: RenameRules = serde_json::from_str(&json).expect("rules should parse");
    assert_eq!(back, rules);
}

#[test]
fn missing_json_fields_fall_back_to_defaults() {
    let rules: RenameRules =
        serde_json::from_str(r#"{"name_case":"title"}"#).expect("partial rules should parse");
    assert_eq!(
        rules,
        RenameRules {
            name_case: NameCase::Title,
            ..RenameRules::default()
        }
    );
}

#[test]
fn json_keeps_the_version_and_snake_case_modes() {
    let json = serde_json::to_value(RenameRules::default()).expect("rules should serialise");
    assert_eq!(json["version"], 1);
    assert_eq!(json["name_case"], "unchanged");
    assert_eq!(json["find"]["first_only"], false);
}
