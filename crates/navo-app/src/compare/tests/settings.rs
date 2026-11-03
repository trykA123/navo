use navo_compare::session::CheckMode;
use navo_git::{
    EngineKind, EngineSetting, GitFound, ObjectFormat, RefStorage, RepoFormat,
    UNSUPPORTED_FORMAT_NOTE,
};

use navo_store::{Compare, ContentCheck, GitEngine};

use crate::compare::settings::{CompareSettings, choose_engine};

fn format(object_format: ObjectFormat) -> RepoFormat {
    RepoFormat {
        git_dir: ".git".into(),
        common_dir: ".git".into(),
        bare: false,
        version: 0,
        object_format,
        ref_storage: RefStorage::Files,
    }
}

fn budget_mib(mib: u32) -> u64 {
    let settings = CompareSettings {
        memory_budget_mib: mib,
        ..CompareSettings::default()
    };
    settings.budget().bytes() / (1024 * 1024)
}

#[test]
fn defaults_match_the_plan() {
    let settings = CompareSettings::default();
    assert!(settings.respect_gitignore);
    assert!(settings.ignore_list.is_empty());
    assert_eq!(settings.git_engine, EngineSetting::Git);
    assert_eq!(budget_mib(settings.memory_budget_mib), 1024);
}

#[test]
fn a_small_budget_is_raised_to_the_minimum() {
    assert_eq!(budget_mib(0), 256);
    assert_eq!(budget_mib(255), 256);
}

#[test]
fn a_large_budget_is_lowered_to_the_maximum() {
    assert_eq!(budget_mib(16_385), 16_384);
    assert_eq!(budget_mib(u32::MAX), 16_384);
}

#[test]
fn a_budget_in_range_is_kept() {
    assert_eq!(budget_mib(256), 256);
    assert_eq!(budget_mib(4096), 4096);
}

#[test]
fn the_git_setting_picks_the_cli_when_git_is_usable() {
    let choice = choose_engine(
        &CompareSettings::default(),
        &format(ObjectFormat::Sha1),
        GitFound::version(2, 40),
    );
    assert_eq!((choice.engine, choice.note), (EngineKind::Cli, None));
}

#[test]
fn the_gix_setting_falls_back_to_the_cli_for_sha256() {
    let settings = CompareSettings {
        git_engine: EngineSetting::Gix,
        ..CompareSettings::default()
    };
    let choice = choose_engine(
        &settings,
        &format(ObjectFormat::Sha256),
        GitFound::version(2, 40),
    );
    assert_eq!(choice.engine, EngineKind::Cli);
    assert_eq!(choice.note, Some(UNSUPPORTED_FORMAT_NOTE));
}

#[test]
fn the_stored_defaults_map_to_the_plain_defaults() {
    assert_eq!(
        CompareSettings::from(&Compare::default()),
        CompareSettings::default()
    );
}

#[test]
fn every_stored_value_maps_across() {
    let stored = Compare {
        git_engine: GitEngine::Gix,
        ignore_list: vec!["*.o".to_owned()],
        respect_gitignore: false,
        memory_budget_mib: 4096,
        content_check: ContentCheck::SizeTime,
    };
    let plain = CompareSettings::from(&stored);
    assert_eq!(plain.git_engine, EngineSetting::Gix);
    assert_eq!(plain.ignore_list, ["*.o"]);
    assert!(!plain.respect_gitignore);
    assert_eq!(plain.memory_budget_mib, 4096);
    assert_eq!(plain.content_check, CheckMode::SizeTime);
    let auto = Compare {
        git_engine: GitEngine::Auto,
        ..Compare::default()
    };
    assert_eq!(CompareSettings::from(&auto).git_engine, EngineSetting::Auto);
}
