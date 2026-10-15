use proptest::prelude::*;

use super::*;

fn wide_path() -> impl Strategy<Value = ProviderPath> {
    prop::collection::vec(any::<u16>(), 0..24).prop_map(|units| ProviderPath::from_wide(&units))
}

fn byte_path() -> impl Strategy<Value = ProviderPath> {
    prop::collection::vec(any::<u8>(), 0..24).prop_map(|bytes| ProviderPath::from_bytes(&bytes))
}

fn any_path() -> impl Strategy<Value = ProviderPath> {
    prop_oneof![wide_path(), byte_path()]
}

fn location_path() -> impl Strategy<Value = ProviderPath> {
    let wide = prop::collection::vec(1..=u16::MAX, 1..24).prop_map(|u| ProviderPath::from_wide(&u));
    let bytes =
        prop::collection::vec(1..=u8::MAX, 1..24).prop_map(|b| ProviderPath::from_bytes(&b));
    prop_oneof![wide, bytes]
}

proptest! {
    #[test]
    fn wide_units_survive_the_text_form(units in prop::collection::vec(any::<u16>(), 0..24)) {
        let path = ProviderPath::from_text(&ProviderPath::from_wide(&units).to_text());
        prop_assert_eq!(path.to_wide(), Some(units));
    }

    #[test]
    fn bytes_survive_the_text_form(bytes in prop::collection::vec(any::<u8>(), 0..24)) {
        let path = ProviderPath::from_text(&ProviderPath::from_bytes(&bytes).to_text());
        prop_assert_eq!(path.to_bytes(), Some(bytes));
    }

    #[test]
    fn paths_survive_the_text_form(path in any_path()) {
        prop_assert_eq!(ProviderPath::from_text(&path.to_text()), path);
    }

    #[test]
    fn local_locations_survive_their_text_form(path in location_path()) {
        if let Ok(location) = Location::local(path) {
            prop_assert_eq!(Location::new(location.as_str()), Ok(location));
        }
    }

    #[test]
    fn local_locations_are_refused_only_when_their_text_names_a_provider(
        id in "[a-z][a-z0-9-]{1,8}",
        rest in "[a-z]{1,8}",
    ) {
        let path = ProviderPath::from_text(&format!("{id}://{rest}"));
        prop_assert_eq!(Location::local(path), Err(LocationError::BadProvider));
    }

    #[test]
    fn provider_locations_survive_their_text_form(path in location_path()) {
        let archive = ProviderId::new("archive").unwrap();
        let location = Location::with_provider(archive, path).unwrap();
        prop_assert_eq!(Location::new(location.as_str()), Ok(location));
    }

    #[test]
    fn windows_eq_is_reflexive(path in any_path()) {
        prop_assert!(path.windows_eq(&path));
    }

    #[test]
    fn windows_eq_is_symmetric(a in any_path(), b in any_path()) {
        prop_assert_eq!(a.windows_eq(&b), b.windows_eq(&a));
    }

    #[test]
    fn windows_eq_is_symmetric_for_text(a in "[aAßσςΣёЁ]{0,3}", b in "[aAßσςΣёЁ]{0,3}") {
        let (a, b) = (ProviderPath::from_text(&a), ProviderPath::from_text(&b));
        prop_assert_eq!(a.windows_eq(&b), b.windows_eq(&a));
    }
}
