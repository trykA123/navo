const DBT_DEVICEARRIVAL: usize = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;
const DBT_DEVNODES_CHANGED: usize = 0x0007;
const DBT_DEVTYP_VOLUME: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    VolumeChange,
}

#[must_use]
pub fn device_event(wparam: usize, device_type: Option<u32>) -> Option<DeviceEvent> {
    let is_volume = device_type == Some(DBT_DEVTYP_VOLUME);
    let is_change = reads_device_header(wparam);
    (wparam == DBT_DEVNODES_CHANGED || (is_volume && is_change))
        .then_some(DeviceEvent::VolumeChange)
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{VolumeWatch, watch_volumes};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{VolumeWatch, watch_volumes};

#[cfg(all(unix, not(target_os = "linux")))]
pub struct VolumeWatch;

#[cfg(all(unix, not(target_os = "linux")))]
#[must_use]
pub fn watch_volumes(_on_change: impl Fn() + Send + 'static) -> Option<VolumeWatch> {
    None
}

fn reads_device_header(wparam: usize) -> bool {
    matches!(wparam, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE)
}

#[cfg(test)]
mod tests {
    use super::{DeviceEvent, device_event, reads_device_header};

    #[test]
    fn a_volume_arriving_or_leaving_is_a_volume_change() {
        assert_eq!(
            device_event(0x8000, Some(2)),
            Some(DeviceEvent::VolumeChange)
        );
        assert_eq!(
            device_event(0x8004, Some(2)),
            Some(DeviceEvent::VolumeChange)
        );
    }

    #[test]
    fn only_arrival_and_removal_can_carry_a_device_header() {
        assert!(reads_device_header(0x8000));
        assert!(reads_device_header(0x8004));
        for event in [0x0007, 0x0048, 0x8001, 0x8003, usize::MAX] {
            assert!(!reads_device_header(event));
        }
    }

    #[test]
    fn device_node_changes_signal_without_a_device_header() {
        assert_eq!(device_event(0x0007, None), Some(DeviceEvent::VolumeChange));
    }

    #[test]
    fn other_devices_and_events_are_ignored() {
        assert_eq!(device_event(0x8000, Some(5)), None);
        assert_eq!(device_event(0x8004, None), None);
        assert_eq!(device_event(0x8001, Some(2)), None);
        assert_eq!(device_event(0x8003, Some(2)), None);
    }
}
