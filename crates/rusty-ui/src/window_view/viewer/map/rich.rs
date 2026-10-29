use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::FocusHandle;
use rusty_automotive::space::{GroupBy, Grouping, Origin, OwnerRef, Owners, Region, lit_blocks};

use super::loaded::Loaded;
use super::select::{Column, Order, Table};
use super::{Focus, Index, MapView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, spawn_job};
use crate::window_view::viewer::body::{Host, Key};
use crate::window_view::viewer::parts::LayoutParts;
use crate::window_view::viewer::space::{
    GroupingOptions, Item as StripItem, Label, Part, Pick, RichHost, RichModel, RichSpace,
    StripData, build_map_model, part_keys, regrouped, start_siblings, stem_of,
};

const LARGEST: Order = Order {
    column: Column::Size,
    descending: true,
};

pub(super) fn start_siblings_of(host: &Host, cx: &mut Context<MapView>) -> Job {
    let stem = stem_of(&host.location).map(|stem| stem.with_elf());
    start_siblings(host, stem, cx, |view, siblings, cx| {
        view.siblings = Arc::new(siblings);
        view.start_model(cx);
    })
}

#[derive(Clone)]
struct Names {
    loaded: Arc<Loaded>,
    owners: Arc<Owners>,
}

impl Names {
    fn describe(&self, region: &Region, item: StripItem) -> Label {
        let map = &self.loaded.model;
        match item {
            StripItem::Block(at) => Label {
                name: block_name(&self.loaded, region, at).into(),
                object: None,
            },
            StripItem::Owner(at) => {
                let owner = self.owners.items.get(at);
                let name = owner.map_or("", |owner| self.owners.name_in(owner, None, Some(map)));
                let object = owner.and_then(|owner| self.owners.object(owner));
                Label {
                    name: name.to_owned().into(),
                    object: object.map(|object| object.to_owned().into()),
                }
            }
        }
    }
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

pub(super) fn block_name(loaded: &Loaded, region: &Region, at: usize) -> String {
    match region.blocks.get(at).map(|block| block.owner) {
        Some(OwnerRef::MapSection(index)) => loaded
            .model
            .sections
            .get(index)
            .map_or_else(String::new, |section| section.name.clone()),
        _ => String::new(),
    }
}

impl MapView {
    pub(super) fn start_model(&mut self, cx: &mut Context<Self>) {
        let Index::Ready(loaded) = &self.index else {
            return;
        };
        let (loaded, siblings) = (loaded.clone(), self.siblings.clone());
        let name = self.title.to_string();
        let options = GroupingOptions {
            grouping: self.rich.grouping,
            modules: self.rich.modules.clone(),
        };
        self.model_job = Some(spawn_job(
            cx,
            move |_, send| {
                let model = build_map_model(&loaded.model, &siblings, (&name, options));
                send((loaded, model));
            },
            Self::model_ready,
        ));
    }

    pub(super) fn model_ready(
        &mut self,
        (loaded, model): (Arc<Loaded>, RichModel),
        cx: &mut Context<Self>,
    ) {
        if model.modules != self.rich.modules {
            return;
        }
        let model = self.scope_module(model, &loaded.model, cx);
        let data = strip_data(Names {
            loaded,
            owners: model.owners.clone(),
        });
        let relight = self.rich.install(model, data, cx);
        if let Some(id) = relight {
            let lit = self.lit_for(id);
            self.rich.set_pick(Some(Pick::Group(id)), &lit, cx);
        }
        if self.rich.wants_regroup().is_some() {
            self.start_regroup(cx);
        }
        cx.notify();
    }

    fn start_regroup(&mut self, cx: &mut Context<Self>) {
        let (Index::Ready(loaded), Some(model), Some(grouping)) =
            (&self.index, self.rich.model.clone(), self.rich.grouping)
        else {
            return;
        };
        let loaded = loaded.clone();
        self.regroup_job = Some(spawn_job(
            cx,
            move |_, send| {
                let model = regrouped(&model, Origin::Map(&loaded.model), grouping);
                send((loaded, model));
            },
            Self::model_ready,
        ));
    }

    pub(super) fn apply_modules(&mut self, modules: Vec<String>, cx: &mut Context<Self>) {
        if self.rich.modules.as_ref() == modules {
            return;
        }
        self.rich.modules = modules.into();
        self.regroup_job = None;
        self.start_model(cx);
    }

    pub(super) fn is_rich(&self) -> bool {
        self.layout.is_rich()
    }

    pub(super) fn apply_layout(&mut self, layout: LayoutParts, cx: &mut Context<Self>) {
        let was_rich = self.is_rich();
        self.layout = layout;
        let is_rich = self.is_rich();
        if is_rich && !was_rich && self.table() == Table::Memory {
            self.query.table = Table::Sections;
            self.query.set_order(Table::Sections, LARGEST);
            self.shows_sections_for_rich = true;
            self.reselect(cx);
        } else if was_rich && !is_rich && self.shows_sections_for_rich {
            self.shows_sections_for_rich = false;
            if self.table() == Table::Sections {
                self.query.table = Table::Memory;
                self.query
                    .set_order(Table::Sections, Table::Sections.default_order());
                self.reselect(cx);
            }
        }
        if !is_rich {
            self.clear_pick(cx);
            self.rich.part = Part::Table;
        }
        cx.notify();
    }

    fn reselect(&mut self, cx: &mut Context<Self>) {
        if self.loaded().is_none() {
            return;
        }
        self.rows = Arc::new(Vec::new());
        self.go_address = None;
        self.move_cursor(0, cx);
        self.run_select(cx);
    }

    pub(super) fn rich_keys(&self, cx: &App) -> Option<Option<Vec<Key>>> {
        self.space_layout()?;
        self.rich.model.as_ref()?;
        Some(part_keys(&self.rich, cx))
    }

    pub(super) fn clear_pick(&mut self, cx: &mut Context<Self>) {
        if self.rich.pick.is_some() {
            self.rich.set_pick(None, &[], cx);
        }
    }

    fn lit_for(&self, id: u32) -> Vec<Option<Rc<[bool]>>> {
        let (Index::Ready(loaded), Some(model)) = (&self.index, &self.rich.model) else {
            return Vec::new();
        };
        let Some(group) = model.groups.get(id as usize) else {
            return Vec::new();
        };
        let by = GroupBy {
            grouping: model.grouping,
            modules: &model.modules,
        };
        model
            .regions
            .iter()
            .map(|region| {
                let lit = lit_blocks(
                    &model.owners,
                    Origin::Map(&loaded.model),
                    region,
                    (&by, &group.name),
                );
                Some(lit.into())
            })
            .collect()
    }
}

impl RichHost for MapView {
    fn enter_module(&mut self, id: u32, cx: &mut Context<Self>) -> bool {
        self.open_module(id, cx)
    }

    fn leave_module(&mut self, cx: &mut Context<Self>) -> bool {
        self.close_module(cx)
    }

    fn rich(&mut self) -> &mut RichSpace {
        &mut self.rich
    }

    fn space_layout(&self) -> Option<LayoutParts> {
        let takes = self.is_rich() && self.focus_in == Focus::Content && self.go_to.is_none();
        takes.then_some(self.layout)
    }

    fn table_focus(&self) -> FocusHandle {
        self.focus.clone()
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
        self.close_module(cx);
        self.rich.grouping = Some(grouping);
        self.start_regroup(cx);
    }
}
