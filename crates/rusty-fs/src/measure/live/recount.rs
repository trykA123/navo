#[derive(Debug, Default)]
pub(super) struct Recounts {}

impl Recounts {
    pub(super) fn request(&mut self) -> bool {
        todo!()
    }

    pub(super) fn note_change(&mut self) {
        todo!()
    }

    pub(super) fn finish(&mut self) -> bool {
        todo!()
    }

    pub(super) fn is_running(&self) -> bool {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::Recounts;

    #[test]
    fn a_trigger_when_idle_starts_a_count() {
        let mut recounts = Recounts::default();
        assert!(recounts.request());
        assert!(recounts.is_running());
        assert!(!recounts.finish());
        assert!(!recounts.is_running());
    }

    #[test]
    fn triggers_during_a_count_coalesce_into_one_follow_up() {
        let mut recounts = Recounts::default();
        assert!(recounts.request());
        assert!(!recounts.request());
        assert!(!recounts.request());
        assert!(recounts.finish());
        assert!(recounts.is_running());
        assert!(!recounts.finish());
    }

    #[test]
    fn changes_during_a_count_cause_exactly_one_follow_up() {
        let mut recounts = Recounts::default();
        assert!(recounts.request());
        recounts.note_change();
        assert!(recounts.finish());
        recounts.note_change();
        recounts.note_change();
        assert!(!recounts.finish());
        assert!(!recounts.is_running());
    }

    #[test]
    fn a_trigger_during_a_follow_up_earns_one_more() {
        let mut recounts = Recounts::default();
        assert!(recounts.request());
        recounts.note_change();
        assert!(recounts.finish());
        assert!(!recounts.request());
        assert!(recounts.finish());
        assert!(!recounts.finish());
    }

    #[test]
    fn changes_while_idle_start_nothing() {
        let mut recounts = Recounts::default();
        recounts.note_change();
        assert!(!recounts.is_running());
        assert!(recounts.request());
        assert!(!recounts.finish());
    }
}
