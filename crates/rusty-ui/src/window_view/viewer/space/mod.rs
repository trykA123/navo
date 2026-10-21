mod bars;
mod facts;
mod keys;
mod layout;
mod model;
mod rich;
mod sibling;
mod strip;
mod switch;
mod treemap;

pub use strip::{Item, Label, MemoryStrip, Step, StripData, StripEvent};
pub use treemap::{Treemap, TreemapEvent, TreemapGroup, TreemapPart};

pub use bars::{FileBytes, FileBytesBar, FilePart, Legend, RegionHead, UsageBar, UsageLine};
pub(in crate::window_view::viewer) use facts::{
    Picked, pick_facts, share_cell, share_head, share_of,
};
pub(in crate::window_view::viewer) use keys::{
    NEXT_PART, Part, bind_keys, host_context, part_keys, space_keys,
};
pub(in crate::window_view::viewer) use layout::{RichLook, rich_layout};
#[cfg(test)]
pub(in crate::window_view::viewer) use model::tests::DEMO_MDF;
pub(in crate::window_view::viewer) use model::{RichModel, build_map_model, build_model};
pub(in crate::window_view::viewer) use rich::{Pick, RichHost, RichSpace};
pub(in crate::window_view::viewer) use sibling::{Siblings, start_siblings, stem_of};
