use std::collections::{BTreeMap, BTreeSet};

use rusty_pdf::protocol::PdfError;

use super::jobs::Job;

const DEATHS_BEFORE_REFUSING: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Verdict {
    Relock,
    Refuse(PdfError),
}

#[derive(Default)]
pub(super) struct Trouble {
    pub broken: BTreeMap<u32, PdfError>,
    pub unread: BTreeSet<u32>,
    retried: BTreeSet<(bool, u32)>,
    deaths: u32,
}

fn is_transient(error: &PdfError) -> bool {
    matches!(error, PdfError::Timeout | PdfError::HelperDied)
}

impl Trouble {
    pub(super) fn succeeded(&mut self) {
        self.deaths = 0;
    }

    pub(super) fn failed(&mut self, job: Job, error: PdfError) -> Option<Verdict> {
        if matches!(error, PdfError::NeedsPassword | PdfError::WrongPassword) {
            return Some(Verdict::Relock);
        }
        if error == PdfError::HelperDied {
            self.deaths += 1;
            if self.deaths >= DEATHS_BEFORE_REFUSING {
                return Some(Verdict::Refuse(error));
            }
        }
        let is_text = matches!(job, Job::Text { .. });
        let page = job.page();
        if is_transient(&error) && self.retried.insert((is_text, page)) {
            return None;
        }
        if is_text {
            self.unread.insert(page);
        } else {
            self.broken.insert(page, error);
        }
        None
    }

    pub(super) fn forget_drawing(&mut self) {
        self.broken.clear();
        self.retried.retain(|(is_text, _)| *is_text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(page: u32) -> Job {
        Job::Page { page, width: 100 }
    }

    #[test]
    fn a_transient_failure_is_retried_once_then_kept() {
        let mut trouble = Trouble::default();
        assert_eq!(trouble.failed(page(2), PdfError::Timeout), None);
        assert!(trouble.broken.is_empty());
        assert_eq!(trouble.failed(page(2), PdfError::Timeout), None);
        assert_eq!(trouble.broken.get(&2), Some(&PdfError::Timeout));
        trouble.forget_drawing();
        assert!(trouble.broken.is_empty());
        assert_eq!(trouble.failed(page(2), PdfError::Corrupt), None);
        assert_eq!(trouble.broken.get(&2), Some(&PdfError::Corrupt));
    }

    #[test]
    fn three_helper_deaths_in_a_row_refuse_the_document() {
        let mut trouble = Trouble::default();
        assert_eq!(trouble.failed(page(1), PdfError::HelperDied), None);
        trouble.succeeded();
        assert_eq!(trouble.failed(page(2), PdfError::HelperDied), None);
        assert_eq!(trouble.failed(page(3), PdfError::HelperDied), None);
        let last = trouble.failed(page(4), PdfError::HelperDied);
        assert_eq!(last, Some(Verdict::Refuse(PdfError::HelperDied)));
    }

    #[test]
    fn a_lost_password_asks_again_and_failed_text_is_remembered() {
        let mut trouble = Trouble::default();
        assert_eq!(
            trouble.failed(page(0), PdfError::NeedsPassword),
            Some(Verdict::Relock)
        );
        trouble.failed(Job::Text { page: 5 }, PdfError::Corrupt);
        assert!(trouble.unread.contains(&5));
    }
}
