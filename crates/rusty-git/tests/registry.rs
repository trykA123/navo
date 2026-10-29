use rusty_git::{EngineRegistry, GitError};

#[test]
fn engines_keep_their_registration_order() {
    let mut registry = EngineRegistry::new();
    registry.register("git", "Git", |_| unreachable!()).unwrap();
    registry.register("gix", "gix", |_| unreachable!()).unwrap();
    let ids: Vec<String> = registry.list().map(|engine| engine.id.clone()).collect();
    assert_eq!(ids, vec!["git".to_owned(), "gix".to_owned()]);
}

#[test]
fn a_duplicate_engine_id_is_refused() {
    let mut registry = EngineRegistry::new();
    registry.register("git", "Git", |_| unreachable!()).unwrap();
    let error = registry
        .register("git", "Git again", |_| unreachable!())
        .unwrap_err();
    assert!(matches!(error, GitError::DuplicateEngine { id } if id == "git"));
}

#[test]
fn an_engine_can_be_looked_up_by_id() {
    let mut registry = EngineRegistry::new();
    registry.register("git", "Git", |_| unreachable!()).unwrap();
    let engine = registry.get("git").unwrap();
    assert_eq!(engine.info().label, "Git");
    assert!(registry.get("gix").is_none());
}
