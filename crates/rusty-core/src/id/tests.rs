use super::*;

#[test]
fn ids_keep_their_raw_value() {
    assert_eq!(WindowId::new(1).get(), 1);
    assert_eq!(PaneId::new(2).get(), 2);
    assert_eq!(TabId::new(u64::MAX).get(), u64::MAX);
}

#[test]
fn ids_order_by_raw_value() {
    assert!(PaneId::new(3) < PaneId::new(4));
}
