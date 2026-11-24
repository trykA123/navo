//! Command table for the Space view (disk-usage map): its keys and palette entries.
//! Split out of `viewer.rs`; `commands/mod.rs` lists it right after the viewer groups.

use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "Space",
    commands: &[
        (
            "space_view::NextPart",
            "space.next_part",
            "cmd-space-next-part",
        ),
        (
            "space_view::PreviousPart",
            "space.previous_part",
            "cmd-space-previous-part",
        ),
        (
            "space_view::PreviousItem",
            "space.previous_item",
            "cmd-space-previous-item",
        ),
        (
            "space_view::NextItem",
            "space.next_item",
            "cmd-space-next-item",
        ),
        (
            "space_view::RegionAbove",
            "space.region_above",
            "cmd-space-region-above",
        ),
        (
            "space_view::RegionBelow",
            "space.region_below",
            "cmd-space-region-below",
        ),
        (
            "space_view::FirstBlock",
            "space.first_block",
            "cmd-space-first-block",
        ),
        (
            "space_view::LastBlock",
            "space.last_block",
            "cmd-space-last-block",
        ),
        ("space_view::ZoomIn", "space.zoom_in", "cmd-space-zoom-in"),
        (
            "space_view::ZoomOut",
            "space.zoom_out",
            "cmd-space-zoom-out",
        ),
        (
            "space_view::PickGroup",
            "space.pick_group",
            "cmd-space-pick-group",
        ),
        (
            "space_view::NextGrouping",
            "space.next_grouping",
            "cmd-space-next-grouping",
        ),
        (
            "space_view::ToggleScale",
            "space.toggle_scale",
            "cmd-space-toggle-scale",
        ),
    ],
}];
