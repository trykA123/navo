use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::Global;
use rusty_automotive::MapModel;
use rusty_automotive::space::{Grouping, Origin};
use rusty_core::Location;

use super::MapView;
use crate::prelude::*;
use crate::window_view::viewer::space::{Pick, RichHost, RichModel, regrouped};

#[derive(Default)]
struct Modules(HashMap<Location, String>);

impl Global for Modules {}

pub(super) fn recall(location: &Location, cx: &mut App) -> Option<String> {
    cx.default_global::<Modules>().0.get(location).cloned()
}

impl MapView {
    fn remember_module(&self, cx: &mut App) {
        let modules = &mut cx.default_global::<Modules>().0;
        if let Some(name) = &self.rich.module {
            modules.insert(self.location.clone(), name.clone());
        } else {
            modules.remove(&self.location);
        }
    }

    pub(super) fn scope_module(
        &mut self,
        model: RichModel,
        map: &MapModel,
        cx: &mut Context<Self>,
    ) -> RichModel {
        let Some(name) = &self.rich.module else {
            return model;
        };
        let base = regrouped(&model, Origin::Map(map), Grouping::Module);
        let Some(module) = base.groups.iter().find(|group| group.name == *name) else {
            self.rich.module = None;
            self.module_model = None;
            self.remember_module(cx);
            return model;
        };
        let mut scoped = regrouped(&base, Origin::Map(map), Grouping::ObjectFile);
        scoped
            .groups
            .retain(|file| module.children.iter().any(|child| child.name == file.name));
        self.module_model = Some(Arc::new(base));
        scoped
    }

    pub(super) fn open_module(&mut self, id: u32, cx: &mut Context<Self>) -> bool {
        let (Some(model), Some(loaded)) = (self.rich.model.clone(), self.loaded().cloned()) else {
            return false;
        };
        if self.rich.module.is_some() || model.grouping != Grouping::Module {
            return false;
        }
        let Some(module) = model.groups.get(id as usize) else {
            return false;
        };
        self.rich.module = Some(module.name.clone());
        self.remember_module(cx);
        self.model_ready((loaded, (*model).clone()), cx);
        true
    }

    pub(super) fn close_module(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(name), Some(model), Some(loaded)) = (
            self.rich.module.clone(),
            self.module_model.clone(),
            self.loaded().cloned(),
        ) else {
            return false;
        };
        self.rich.module = None;
        self.module_model = None;
        let id = model
            .groups
            .iter()
            .position(|group| group.name == name)
            .map(|at| at as u32);
        self.remember_module(cx);
        self.model_ready((loaded, (*model).clone()), cx);
        if let Some(id) = id {
            self.rich.tile = Some(id);
            self.picked(Pick::Group(id), cx);
        }
        true
    }
}
