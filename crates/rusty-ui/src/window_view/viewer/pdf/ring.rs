use rusty_pdf::protocol::LinkTarget;

use super::PdfView;
use super::doc::Doc;
use crate::prelude::*;

pub(super) type Ring = (u32, usize);

fn in_reading_order(doc: &Doc) -> Vec<Ring> {
    let mut found: Vec<(Ring, f32, f32)> = doc
        .shown()
        .filter_map(|page| Some((page, doc.links_on(page)?)))
        .flat_map(|(page, links)| {
            links
                .links()
                .iter()
                .enumerate()
                .map(move |(index, link)| ((page, index), link.bounds.top, link.bounds.left))
        })
        .collect();
    found.sort_by(|a, b| {
        a.0.0
            .cmp(&b.0.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    found.into_iter().map(|(ring, ..)| ring).collect()
}

fn next_in(order: &[Ring], at: Option<Ring>, backward: bool) -> Option<Ring> {
    let count = order.len();
    let here = at.and_then(|ring| order.iter().position(|found| *found == ring));
    let to = match (here, backward) {
        (None, false) => 0,
        (None, true) => count.checked_sub(1)?,
        (Some(at), false) => (at + 1) % count,
        (Some(at), true) => at.checked_sub(1).unwrap_or(count - 1),
    };
    order.get(to).copied()
}

impl PdfView {
    fn ring_free(&self) -> bool {
        self.drawers.is_empty() && !self.is_finding
    }

    pub(super) fn step_ring(&mut self, backward: bool, cx: &mut Context<Self>) {
        if !self.ring_free() {
            return cx.propagate();
        }
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let Some((page, index)) = next_in(&in_reading_order(doc), doc.ring, backward) else {
            return;
        };
        doc.ring = Some((page, index));
        if let Some(link) = doc
            .links_on(page)
            .and_then(|links| links.links().get(index))
        {
            let band = link.bounds.top..link.bounds.bottom;
            doc.camera.show_band(&doc.pages, page, band);
        }
        cx.notify();
    }

    pub(super) fn follow_ring(&mut self, cx: &mut Context<Self>) {
        match self.ringed_target() {
            Some(target) => self.follow(target, cx),
            None => cx.propagate(),
        }
    }

    fn ringed_target(&self) -> Option<LinkTarget> {
        if !self.ring_free() {
            return None;
        }
        let doc = self.doc()?;
        let (page, index) = doc.ring?;
        let link = doc.links_on(page)?.links().get(index)?;
        Some(link.target.clone())
    }

    pub(super) fn clear_ring(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.ring_free() {
            return false;
        }
        let cleared = self.doc_mut().is_some_and(|doc| doc.ring.take().is_some());
        if cleared {
            cx.notify();
        }
        cleared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_wrap_and_start_at_the_ends() {
        let order = [(0, 0), (0, 1), (1, 0)];
        assert_eq!(next_in(&order, None, false), Some((0, 0)));
        assert_eq!(next_in(&order, None, true), Some((1, 0)));
        assert_eq!(next_in(&order, Some((1, 0)), false), Some((0, 0)));
        assert_eq!(next_in(&order, Some((0, 0)), true), Some((1, 0)));
        assert_eq!(next_in(&order, Some((5, 5)), false), Some((0, 0)));
        assert_eq!(next_in(&[], None, true), None);
    }
}
