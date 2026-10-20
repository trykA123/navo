use std::rc::Rc;
use std::sync::Arc;

use rusty_automotive::space::{GroupBy, Grouping, Origin, OwnerRef, Owners, Region, lit_blocks};
use rusty_automotive::{Column, Elf, MapModel, Order};

use super::{ElfView, Index};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, spawn_job};
use crate::window_view::viewer::body::Host;
use crate::window_view::viewer::parts::LayoutParts;
use crate::window_view::viewer::space::{
    Item as StripItem, Label, Pick, RichHost, RichModel, RichSpace, StripData, build_model,
    start_siblings, stem_of,
};

const LARGEST: Order = Order {
    column: Column::Size,
    descending: true,
};

pub(super) fn start_siblings_of(host: &Host, cx: &mut Context<ElfView>) -> Job {
    start_siblings(host, stem_of(&host.location), cx, |view, siblings, cx| {
        view.siblings = Arc::new(siblings);
        view.start_model(cx);
    })
}

#[derive(Clone)]
struct Names {
    elf: Arc<Elf>,
    owners: Arc<Owners>,
    map: Option<Arc<MapModel>>,
}

fn strip_data(names: Names) -> impl Fn(&Arc<Region>) -> StripData {
    move |region| {
        let (names, held) = (names.clone(), region.clone());
        StripData {
            region: region.clone(),
            owners: Some(names.owners.clone()),
            describe: Rc::new(move |item| names.describe(&held, item)),
        }
    }
}

impl Names {
    fn describe(&self, region: &Region, item: StripItem) -> Label {
        let Self { elf, owners, map } = self;
        match item {
            StripItem::Block(at) => Label {
                name: block_name(elf, region, at).into(),
                object: None,
            },
            StripItem::Owner(at) => {
                let owner = owners.items.get(at);
                let name = owner.map_or("", |owner| owners.name(owner, elf, map.as_deref()));
                let object = owner.and_then(|owner| owners.object(owner));
                Label {
                    name: name.to_owned().into(),
                    object: object.map(|object| object.to_owned().into()),
                }
            }
        }
    }
}

pub(super) fn block_name(elf: &Elf, region: &Region, at: usize) -> String {
    let section = |index: usize| {
        elf.sections
            .get(index)
            .map_or_else(String::new, |s| s.name.clone())
    };
    match region.blocks.get(at).map(|block| block.owner) {
        Some(OwnerRef::ElfSection(index) | OwnerRef::ElfImage(index)) => section(index),
        Some(OwnerRef::MapSection(_)) | None => String::new(),
    }
}

impl ElfView {
    pub(super) fn start_model(&mut self, cx: &mut Context<Self>) {
        let Index::Ready(elf) = &self.index else {
            return;
        };
        let (elf, lines, siblings) = (elf.clone(), self.lines.clone(), self.siblings.clone());
        let grouping = self.rich.grouping;
        self.model_job = Some(spawn_job(
            cx,
            move |_, send| send(build_model(&elf, lines.as_deref(), &siblings, grouping)),
            Self::model_ready,
        ));
    }

    fn model_ready(&mut self, model: RichModel, cx: &mut Context<Self>) {
        let Index::Ready(elf) = &self.index else {
            return;
        };
        let data = strip_data(Names {
            elf: elf.clone(),
            owners: model.owners.clone(),
            map: self.siblings.map.clone(),
        });
        self.rich.install(model, data, cx);
        cx.notify();
    }

    pub(super) fn is_rich(&self) -> bool {
        self.layout.is_rich()
    }

    pub(super) fn apply_layout(&mut self, layout: LayoutParts, cx: &mut Context<Self>) {
        let was_rich = self.is_rich();
        self.layout = layout;
        let is_rich = self.is_rich();
        if is_rich && !was_rich && self.query.order == Order::default() {
            self.query.order = LARGEST;
            self.auto_sorted = true;
            self.run_select(cx);
        } else if was_rich && !is_rich && self.auto_sorted && self.query.order == LARGEST {
            self.query.order = Order::default();
            self.auto_sorted = false;
            self.run_select(cx);
        }
        if !is_rich {
            self.clear_pick(cx);
        }
        cx.notify();
    }

    pub(super) fn clear_pick(&mut self, cx: &mut Context<Self>) {
        if self.rich.pick.is_some() {
            self.rich.set_pick(None, &[], cx);
        }
    }

    fn lit_for(&self, id: u32) -> Vec<Option<Rc<[bool]>>> {
        let (Index::Ready(elf), Some(model)) = (&self.index, &self.rich.model) else {
            return Vec::new();
        };
        let Some(group) = model.groups.get(id as usize) else {
            return Vec::new();
        };
        let origin = Origin::Elf {
            elf,
            map: self.siblings.map.as_deref(),
        };
        let by = GroupBy {
            grouping: model.grouping,
            modules: &[],
        };
        model
            .regions
            .iter()
            .map(|region| {
                Some(lit_blocks(&model.owners, origin, region, (&by, &group.name)).into())
            })
            .collect()
    }
}

impl RichHost for ElfView {
    fn rich(&mut self) -> &mut RichSpace {
        &mut self.rich
    }

    fn picked(&mut self, pick: Pick, cx: &mut Context<Self>) {
        let lit = match pick {
            Pick::Group(id) => self.lit_for(id),
            Pick::Block { .. } => Vec::new(),
        };
        self.rich.set_pick(Some(pick), &lit, cx);
        self.open_inspector_if_asked(cx);
        cx.notify();
    }

    fn regroup(&mut self, grouping: Grouping, cx: &mut Context<Self>) {
        self.rich.grouping = Some(grouping);
        self.clear_pick(cx);
        self.start_model(cx);
    }
}
