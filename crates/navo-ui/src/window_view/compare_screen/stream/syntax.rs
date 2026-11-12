//! The Stream's text-card syntax parses (D38.3): one background job per held text card, keyed by
//! the card's id and the Stream generation it arrived under. A card that is evicted or replaced
//! loses its parse, and a result that lands for another card is dropped.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{AsyncApp, Task, WeakEntity};

use super::{StreamScreen, item_of};
use crate::compare_diff::card::syntax::{CardKey, CardLook, CardSyntax};
use crate::compare_diff::{PairKind, StreamModel, StreamUpdate};
use crate::prelude::*;
use crate::window_view::{EDIT_LIMIT, NavoWindow};

/// One card's parse job: its key and the path the language is detected from.
struct Job {
    key: CardKey,
    path: String,
}

/// One card's slot: the key the job was started for, the job waiting and the finished snippets.
/// Dropping the slot drops the job with it.
struct Slot {
    key: CardKey,
    task: Option<Task<()>>,
    ready: Option<Rc<CardSyntax>>,
}

/// The Stream's card parses, by item id. As visible as the `StreamScreen` field that holds it.
#[derive(Default)]
pub(super) struct StreamSyntax {
    slots: HashMap<u64, Slot>,
}

impl StreamSyntax {
    /// The finished snippets of item `id`'s card, `None` while it holds none.
    fn ready(&self, id: u64) -> Option<&CardSyntax> {
        self.slots.get(&id)?.ready.as_deref()
    }

    /// Drops the slots of cards that are gone, replaced or read under an older generation, and
    /// returns those the Stream still holds, so they are parsed again (D38.3).
    fn prune(&mut self, model: &StreamModel) -> Vec<u64> {
        let generation = model.generation();
        let mut again = Vec::new();
        self.slots.retain(|id, slot| {
            let held = slot.key.held(model);
            if held && slot.key.stale(generation) {
                again.push(*id);
            }
            held && !slot.key.stale(generation)
        });
        again
    }

    /// The jobs for `ids`: the held text cards among them that have no slot yet. Any other kind
    /// keeps drawing as before (D38.3).
    fn jobs(&self, model: &StreamModel, ids: &[u64]) -> Vec<Job> {
        ids.iter()
            .filter(|id| !self.slots.contains_key(*id))
            .filter_map(|id| job(model, *id))
            .collect()
    }

    /// Starts one background job per card, the whole card in one job since its snippets are small.
    /// The slot keeps the job, so a result is checked against the card held when it ends.
    fn start(&mut self, jobs: Vec<Job>, cx: &mut Context<NavoWindow>) {
        for job in jobs {
            let key = job.key.clone();
            let card = Arc::clone(job.key.card());
            let path = job.path;
            let parsing =
                cx.background_spawn(async move { CardSyntax::parse(&path, &card, EDIT_LIMIT) });
            let waiting =
                cx.spawn(async move |this, cx| deliver_later(key, parsing, this, cx).await);
            self.slots.insert(
                job.key.id(),
                Slot {
                    key: job.key,
                    task: Some(waiting),
                    ready: None,
                },
            );
        }
    }

    /// Keeps `parsed` on the slot it was started from and lets go of the waiting job. Returns
    /// whether the Stream still wanted it (D38.3).
    fn deliver(&mut self, model: &StreamModel, key: CardKey, parsed: CardSyntax) -> bool {
        if !key.held(model) {
            return false;
        }
        let Some(slot) = self.slots.get_mut(&key.id()) else {
            return false;
        };
        if !slot.key.same(&key) {
            return false;
        }
        slot.ready = Some(Rc::new(parsed));
        slot.task = None;
        true
    }
}

/// Waits for one card's parse and hands the result to the window; the window drops it when the
/// card it was read from is no longer held (D38.3).
async fn deliver_later(
    key: CardKey,
    parsing: Task<CardSyntax>,
    this: WeakEntity<NavoWindow>,
    cx: &mut AsyncApp,
) {
    let parsed = parsing.await;
    let _ = this.update(cx, |window, cx| {
        window.deliver_stream_syntax(key, parsed, cx);
    });
}

/// The job of item `id`: its held text card and the path of the listed item it belongs to.
fn job(model: &StreamModel, id: u64) -> Option<Job> {
    let card = model.card(id)?;
    if card.kind != PairKind::Text {
        return None;
    }
    let path = model
        .items()
        .iter()
        .find(|item| item.id == id)?
        .path
        .clone();
    Some(Job {
        key: CardKey::new(id, model.generation(), card),
        path,
    })
}

/// The items whose card the poll applied under the generation the model holds now.
fn arrived(updates: &[StreamUpdate], generation: u64) -> Vec<u64> {
    updates
        .iter()
        .filter_map(|update| match update {
            StreamUpdate::Card {
                id,
                generation: sent,
                ..
            } if *sent == generation => Some(*id),
            _ => None,
        })
        .collect()
}

impl StreamScreen {
    /// Prunes the parses of cards that are gone, replaced or re-listed, and starts a job for every
    /// text card that arrived or needs one (D38.3). The caller has just applied the updates to the
    /// model, so a card that left with them takes its parse along.
    pub(super) fn sync_syntax(&mut self, updates: &[StreamUpdate], cx: &mut Context<NavoWindow>) {
        let mut ids = self.syntax.prune(&self.model);
        ids.extend(arrived(updates, self.model.generation()));
        let jobs = self.syntax.jobs(&self.model, &ids);
        self.syntax.start(jobs, cx);
    }

    /// The syntax the rows of the card of list row `at` draw with: the parse of the card the
    /// Stream holds for it and the theme's live highlight colours; `None` while no parse has
    /// landed, the row is not a card row or the theme has no colours (D38.3).
    pub(super) fn card_syntax<'a>(&'a self, at: usize, cx: &'a App) -> Option<CardLook<'a>> {
        let item = item_of(self.rows.get(at)?)?;
        let id = self.model.items().get(item)?.id;
        CardLook::new(self.syntax.ready(id), colours(cx))
    }
}

/// The theme's syntax highlight colours as F3 reads them; `None` before a theme is set.
fn colours(cx: &App) -> Option<&HighlightTheme> {
    let kit = cx.try_global::<KitTheme>()?;
    Some(&kit.highlight_theme)
}

impl NavoWindow {
    /// Delivers a card parse that just ended: the Stream keeps it only while it still holds that
    /// very card under that generation, and then redraws (D38.3). Returns whether it was kept.
    pub(in crate::window_view) fn deliver_stream_syntax(
        &mut self,
        key: CardKey,
        parsed: CardSyntax,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(stream) = self.stream_mut() else {
            return false;
        };
        let kept = stream.syntax.deliver(&stream.model, key, parsed);
        if kept {
            cx.notify();
        }
        kept
    }
}

#[cfg(test)]
impl NavoWindow {
    /// The parse the Stream holds for item `id`'s card, once its background job ended.
    pub(in crate::window_view) fn stream_syntax(&self, id: u64) -> Option<Rc<CardSyntax>> {
        let stream = self.compare.screen.as_ref()?.stream.as_ref()?;
        stream.syntax.slots.get(&id)?.ready.clone()
    }

    /// How many card parse slots the open Stream holds; none when no Stream is open.
    pub(in crate::window_view) fn stream_syntax_slots(&self) -> usize {
        self.compare
            .screen
            .as_ref()
            .and_then(|screen| screen.stream.as_ref())
            .map_or(0, |stream| stream.syntax.slots.len())
    }
}
