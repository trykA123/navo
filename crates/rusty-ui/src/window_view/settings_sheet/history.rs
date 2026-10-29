use super::FerriteWindow;

impl FerriteWindow {
    pub(super) fn clear_search_history(&mut self) {
        let clear = self.settings.search.clear.clone();
        self.settings.is_history_cleared = false;
        self.lane.push(Box::new(move || {
            clear();
            Box::new(|view, _, _| {
                view.settings.is_history_cleared = true;
            })
        }));
    }
}
