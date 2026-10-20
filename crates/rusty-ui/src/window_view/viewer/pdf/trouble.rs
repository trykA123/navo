use std::collections::{BTreeMap, BTreeSet};

use rusty_pdf::protocol::PdfError;

use super::jobs::Job;

const DEATHS_BEFORE_REFUSING: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Verdict {
    Relock,
    Refuse(PdfError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Drawing,
    Text,
    Links,
    Outline,
}

impl Kind {
    fn of(job: Job) -> Self {
        match job {
            Job::Page { .. } | Job::Thumb { .. } => Self::Drawing,
            Job::Text { .. } => Self::Text,
            Job::Links { .. } => Self::Links,
            Job::Outline => Self::Outline,
        }
    }
}

#[derive(Default)]
pub(super) struct Trouble {
    pub broken: BTreeMap<u32, PdfError>,
    pub unread: BTreeSet<u32>,
    pub unlinked: BTreeSet<u32>,
    pub outline_lost: bool,
    retried: BTreeSet<(Kind, u32)>,
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
        let (kind, page) = (Kind::of(job), job.page());
        if is_transient(&error) && self.retried.insert((kind, page)) {
            return None;
        }
        match kind {
            Kind::Drawing => {
                self.broken.insert(page, error);
            }
            Kind::Text => {
                self.unread.insert(page);
            }
            Kind::Links => {
                self.unlinked.insert(page);
            }
            Kind::Outline => self.outline_lost = true,
        }
        None
    }

    pub(super) fn forget_drawing(&mut self) {
        self.broken.clear();
        self.retried.retain(|(kind, _)| *kind != Kind::Drawing);
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

    #[test]
    fn link_and_outline_failures_never_break_drawing_and_retry_once() {
        let mut trouble = Trouble::default();
        assert_eq!(
            trouble.failed(Job::Links { page: 0 }, PdfError::Timeout),
            None
        );
        assert!(trouble.unlinked.is_empty());
        trouble.failed(Job::Links { page: 0 }, PdfError::Timeout);
        assert!(trouble.unlinked.contains(&0));
        trouble.failed(Job::Outline, PdfError::Timeout);
        assert!(!trouble.outline_lost);
        trouble.failed(Job::Outline, PdfError::Timeout);
        assert!(trouble.outline_lost);
        assert!(trouble.broken.is_empty());
    }

    #[test]
    fn helper_deaths_on_links_and_outline_count_toward_refusing() {
        let mut trouble = Trouble::default();
        assert_eq!(
            trouble.failed(Job::Links { page: 1 }, PdfError::HelperDied),
            None
        );
        assert_eq!(trouble.failed(Job::Outline, PdfError::HelperDied), None);
        let last = trouble.failed(Job::Links { page: 2 }, PdfError::HelperDied);
        assert_eq!(last, Some(Verdict::Refuse(PdfError::HelperDied)));
    }
}
