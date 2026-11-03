//! One shared bound for user-started recursive walks on a network path (D103).

pub use navo_core::{
    NETWORK_WALK_MAX_ENTRIES, NETWORK_WALK_MAX_TIME, NetworkStop, NetworkWalkBudget,
};

/// True when a walk must run under a `NetworkWalkBudget`: a network drive or a UNC path.
#[must_use]
pub fn needs_network_bound(class: crate::DriveClass) -> bool {
    matches!(class, crate::DriveClass::Network)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_network_class_needs_the_bound() {
        assert!(needs_network_bound(crate::DriveClass::Network));
        assert!(!needs_network_bound(crate::DriveClass::Ssd));
    }
}
