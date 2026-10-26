use rusty_automotive::space::{GroupBy, Grouping, Origin, group};

use super::model::RichModel;

pub(in crate::window_view::viewer) fn regrouped(
    model: &RichModel,
    origin: Origin<'_>,
    wanted: Grouping,
) -> RichModel {
    let grouping = if model.usable[wanted as usize] {
        wanted
    } else {
        model.grouping
    };
    let by = GroupBy {
        grouping,
        modules: &model.modules,
    };
    let region = model.flash.and_then(|at| model.regions.get(at));
    RichModel {
        regions: model.regions.clone(),
        outside: model.outside,
        owners: model.owners.clone(),
        flash: model.flash,
        grouping,
        usable: model.usable,
        groups: region.map_or_else(Vec::new, |region| group(&model.owners, origin, region, &by)),
        provenance: model.provenance.clone(),
        region_source: model.region_source.clone(),
        stripped: model.stripped,
        from_map: model.from_map,
        boundaries: model.boundaries.clone(),
        modules: model.modules.clone(),
    }
}
