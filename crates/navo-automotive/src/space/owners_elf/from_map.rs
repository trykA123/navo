use std::ops::Range;

use crate::map::Contribution;
use crate::map::MapModel;

pub(super) struct MapIndex<'a> {
    map: &'a MapModel,
    order: Vec<u32>,
}

impl<'a> MapIndex<'a> {
    pub(super) fn new(map: &'a MapModel) -> Self {
        let mut order: Vec<u32> = map
            .contributions
            .iter()
            .enumerate()
            .filter(|(_, part)| part.address.is_some() && part.size > 0)
            .map(|(at, _)| at as u32)
            .collect();
        order.sort_by_key(|at| map.contributions[*at as usize].address);
        Self { map, order }
    }

    pub(super) fn within(
        &self,
        span: Range<u64>,
    ) -> impl Iterator<Item = (u32, &'a Contribution)> + '_ {
        let map = self.map;
        let address = move |at: u32| map.contributions[at as usize].address.unwrap_or(0);
        let from = self.order.partition_point(|at| address(*at) < span.start);
        self.order[from..]
            .iter()
            .copied()
            .take_while(move |at| address(*at) < span.end)
            .map(move |at| (at, &map.contributions[at as usize]))
    }
}
