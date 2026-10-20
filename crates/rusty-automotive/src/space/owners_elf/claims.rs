use std::collections::BTreeMap;
use std::ops::Range;

pub(super) struct Claims {
    span: Range<u64>,
    taken: BTreeMap<u64, u64>,
}

impl Claims {
    pub(super) fn new(span: Range<u64>) -> Self {
        Self {
            span,
            taken: BTreeMap::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.taken.is_empty()
    }

    pub(super) fn claim(&mut self, want: Range<u64>) -> Vec<Range<u64>> {
        let start = want.start.max(self.span.start);
        let end = want.end.min(self.span.end);
        if start >= end {
            return Vec::new();
        }
        let mut cursor = start;
        if let Some((_, taken_end)) = self.taken.range(..=start).next_back() {
            cursor = cursor.max(*taken_end);
        }
        let mut free = Vec::new();
        for (from, to) in self.taken.range(start..end) {
            if *from > cursor {
                free.push(cursor..*from);
            }
            cursor = cursor.max(*to);
        }
        if cursor < end {
            free.push(cursor..end);
        }
        for piece in &free {
            self.taken.insert(piece.start, piece.end);
        }
        free
    }

    pub(super) fn gaps(&self) -> Vec<Range<u64>> {
        let mut gaps = Vec::new();
        let mut cursor = self.span.start;
        for (from, to) in &self.taken {
            if *from > cursor {
                gaps.push(cursor..*from);
            }
            cursor = cursor.max(*to);
        }
        if cursor < self.span.end {
            gaps.push(cursor..self.span.end);
        }
        gaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_clip_to_the_span_and_to_earlier_claims() {
        let mut claims = Claims::new(10..100);
        assert_eq!(claims.claim(0..20), vec![10..20]);
        assert_eq!(claims.claim(15..40), vec![20..40]);
        assert_eq!(claims.claim(50..60), vec![50..60]);
        assert_eq!(claims.claim(30..70), vec![40..50, 60..70]);
        assert!(claims.claim(12..18).is_empty());
        assert_eq!(claims.gaps(), vec![70..100]);
    }

    #[test]
    fn an_empty_claim_set_has_one_gap() {
        let claims = Claims::new(5..9);
        assert!(claims.is_empty());
        assert_eq!(claims.gaps(), vec![5..9]);
    }
}
