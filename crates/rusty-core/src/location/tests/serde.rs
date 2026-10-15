use super::*;

#[test]
fn a_location_round_trips_through_json() {
    let location = Location::local(ProviderPath::from_wide(&[0x43, 0x3A, 0x5C, 0xD800])).unwrap();
    let json = serde_json::to_string(&location).unwrap();
    assert_eq!(serde_json::from_str::<Location>(&json).unwrap(), location);
}

#[test]
fn a_location_serialises_as_its_text_form() {
    let location = loc(r"C:\Users\admin");
    assert_eq!(
        serde_json::to_string(&location).unwrap(),
        r#""C:\\Users\\admin""#
    );
}

#[test]
fn an_invalid_location_is_refused() {
    assert!(serde_json::from_str::<Location>(r#""""#).is_err());
}
