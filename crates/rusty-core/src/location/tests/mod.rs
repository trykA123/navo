use super::*;

mod compare;
mod escape_collision;
mod join;
mod parent;
mod path;
mod props;
mod serde;
mod text_form;

fn loc(text: &str) -> Location {
    Location::new(text).unwrap()
}

#[test]
fn keeps_the_text_unchanged() {
    let text = "C:\\Users\\admin\\ \"quoted\" ünï 📁";
    assert_eq!(
        Location::new(text).map(|l| l.as_str().to_owned()),
        Ok(text.to_owned())
    );
}

#[test]
fn rejects_empty_text() {
    assert_eq!(Location::new(""), Err(LocationError::Empty));
}

#[test]
fn rejects_nul() {
    assert_eq!(Location::new("a\0b"), Err(LocationError::Nul));
}

#[test]
fn provider_root_is_the_slash_path() {
    let id = ProviderId::new("fake").unwrap();
    let root = Location::provider_root(id.clone());
    assert_eq!(
        root,
        Location::with_provider(id, ProviderPath::from_text("/")).unwrap()
    );
}

#[test]
fn local_provider_root_is_slash() {
    assert_eq!(Location::provider_root(ProviderId::local()).as_str(), "/");
}
