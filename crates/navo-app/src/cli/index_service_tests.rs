use super::*;

use crate::index_service::Action;

#[test]
fn index_service_arguments_stand_alone() {
    for (text, action) in [
        ("run", Action::Run),
        ("install", Action::Install),
        ("remove", Action::Remove),
    ] {
        let args = ["--index-service", text].map(Into::into);
        assert_eq!(parse(args).unwrap(), Command::IndexService(action));
    }
    for args in [
        vec!["--index-service"],
        vec!["--index-service", "unknown"],
        vec!["--index-service", "run", "extra"],
        vec!["C:\\", "--index-service", "run"],
    ] {
        assert!(parse(args.into_iter().map(Into::into)).is_err());
    }
}
