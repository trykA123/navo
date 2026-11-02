use std::path::PathBuf;

use rusty_git::{
    CLI_ENGINE_ID, Choice, EngineKind, EngineRegistry, EngineSetting, GIX_ENGINE_ID, GitFound,
    INSTALL_GIT_NOTE, MERGE_BASE_GIX_NOTE, NO_GIT_NOTE, ObjectFormat, RefStorage, RepoFormat,
    UNSUPPORTED_FORMAT_NOTE, Work, choose,
};

fn repo(object_format: ObjectFormat, ref_storage: RefStorage) -> RepoFormat {
    RepoFormat {
        git_dir: PathBuf::from(".git"),
        common_dir: PathBuf::from(".git"),
        bare: false,
        version: 1,
        object_format,
        ref_storage,
    }
}

fn sha1() -> RepoFormat {
    repo(ObjectFormat::Sha1, RefStorage::Files)
}

fn sha256() -> RepoFormat {
    repo(ObjectFormat::Sha256, RefStorage::Files)
}

fn reftable() -> RepoFormat {
    repo(ObjectFormat::Sha1, RefStorage::Reftable)
}

fn ready() -> GitFound {
    GitFound::version(2, 36)
}

fn old() -> GitFound {
    GitFound::version(2, 35)
}

#[test]
fn choose_the_git_setting_uses_the_cli() {
    assert_eq!(
        choose(EngineSetting::Git, &sha1(), ready()),
        Choice {
            engine: EngineKind::Cli,
            note: None
        }
    );
    assert_eq!(
        choose(EngineSetting::Git, &reftable(), ready()).engine,
        EngineKind::Cli
    );
}

#[test]
fn choose_the_git_setting_falls_back_to_gix_without_git() {
    for git in [GitFound::missing(), old()] {
        let choice = choose(EngineSetting::Git, &sha1(), git);
        assert_eq!(choice.engine, EngineKind::Gix);
        assert_eq!(choice.note, Some(NO_GIT_NOTE));
    }
}

#[test]
fn choose_the_git_setting_without_git_asks_for_git_when_gix_cannot_read_the_format() {
    for format in [sha256(), reftable()] {
        let choice = choose(EngineSetting::Git, &format, GitFound::missing());
        assert_eq!(choice.engine, EngineKind::Gix);
        assert_eq!(choice.note, Some(INSTALL_GIT_NOTE));
    }
}

#[test]
fn choose_auto_uses_gix_for_sha1_with_files_references() {
    for git in [ready(), GitFound::missing()] {
        let choice = choose(EngineSetting::Auto, &sha1(), git);
        assert_eq!(choice.engine, EngineKind::Gix);
        assert_eq!(choice.note, None);
    }
}

#[test]
fn choose_auto_uses_the_cli_for_other_formats() {
    for format in [sha256(), reftable()] {
        let choice = choose(EngineSetting::Auto, &format, ready());
        assert_eq!(choice.engine, EngineKind::Cli);
        assert_eq!(choice.note, None);
    }
}

#[test]
fn choose_auto_without_git_asks_for_git_when_gix_cannot_read_the_format() {
    let choice = choose(EngineSetting::Auto, &sha256(), GitFound::missing());
    assert_eq!(choice.engine, EngineKind::Gix);
    assert_eq!(choice.note, Some(INSTALL_GIT_NOTE));
}

#[test]
fn choose_the_gix_setting_uses_gix_for_sha1_with_files_references() {
    let choice = choose(EngineSetting::Gix, &sha1(), ready());
    assert_eq!(choice.engine, EngineKind::Gix);
    assert_eq!(choice.note, None);
}

#[test]
fn choose_the_gix_setting_falls_back_to_the_cli_for_an_unsupported_format() {
    for format in [sha256(), reftable()] {
        let choice = choose(EngineSetting::Gix, &format, ready());
        assert_eq!(choice.engine, EngineKind::Cli);
        assert_eq!(choice.note, Some(UNSUPPORTED_FORMAT_NOTE));
    }
}

#[test]
fn choose_the_gix_setting_without_git_asks_for_git_for_an_unsupported_format() {
    let choice = choose(EngineSetting::Gix, &reftable(), old());
    assert_eq!(choice.engine, EngineKind::Gix);
    assert_eq!(choice.note, Some(INSTALL_GIT_NOTE));
}

#[test]
fn choose_work_gix_does_not_do_goes_to_the_cli() {
    let choice = choose(EngineSetting::Auto, &sha1(), ready());
    assert_eq!(choice.engine, EngineKind::Gix);
    for work in [Work::Filters, Work::WorktreeStatus] {
        let routed = choice.for_work(work, ready());
        assert_eq!(routed.engine, EngineKind::Cli);
        assert_eq!(routed.note, None);
    }
    assert_eq!(choice.for_work(Work::Read, ready()), choice);
}

#[test]
fn choose_merge_base_goes_to_the_cli_for_every_setting() {
    for setting in [EngineSetting::Git, EngineSetting::Auto, EngineSetting::Gix] {
        let routed = choose(setting, &sha1(), ready()).for_work(Work::MergeBase, ready());
        assert_eq!(routed.engine, EngineKind::Cli);
        assert_eq!(routed.note, None);
    }
}

#[test]
fn choose_merge_base_without_git_uses_gix_with_a_note() {
    for setting in [EngineSetting::Git, EngineSetting::Auto, EngineSetting::Gix] {
        let choice = choose(setting, &sha1(), GitFound::missing());
        let routed = choice.for_work(Work::MergeBase, GitFound::missing());
        assert_eq!(routed.engine, EngineKind::Gix);
        assert_eq!(routed.note, Some(MERGE_BASE_GIX_NOTE));
    }
}

#[test]
fn choose_unsupported_work_without_git_asks_for_git() {
    let choice = choose(EngineSetting::Auto, &sha1(), GitFound::missing());
    let routed = choice.for_work(Work::WorktreeStatus, GitFound::missing());
    assert_eq!(routed.engine, EngineKind::Gix);
    assert_eq!(routed.note, Some(INSTALL_GIT_NOTE));
}

#[test]
fn choose_chosen_engine_ids_are_in_the_registry() {
    let mut registry = EngineRegistry::new();
    registry
        .register(CLI_ENGINE_ID, "Git", |_| unreachable!())
        .unwrap();
    registry
        .register(GIX_ENGINE_ID, "gix", |_| unreachable!())
        .unwrap();
    for setting in [EngineSetting::Git, EngineSetting::Gix, EngineSetting::Auto] {
        let choice = choose(setting, &sha1(), ready());
        assert!(registry.get(choice.engine.id()).is_some());
    }
}
