//! The set of plan steps a job already finished, read from its journal.
//! A bitmap (one bit per step number) so the step loop asks "done?" in O(1) time.

use navo_core::OperationPlan;

/// Finished step numbers as a bitmap: bit `seq % 64` of word `seq / 64`.
pub(super) struct DoneSet {
    bits: Vec<u64>,
}

impl DoneSet {
    /// Builds the set from the journal's finished step numbers, in any order, duplicates allowed.
    /// `steps` is the plan's step count; a seq at or past it is ignored, since no step asks for it.
    pub(super) fn of(done: &[u32], steps: usize) -> Self {
        let mut bits = vec![0_u64; steps.div_ceil(64)];
        for &seq in done {
            if let Some(word) = bits.get_mut(word_of(seq))
                && usize::try_from(seq).is_ok_and(|seq| seq < steps)
            {
                *word |= bit_of(seq);
            }
        }
        Self { bits }
    }

    /// Builds the set for `plan` from its journal's finished step numbers.
    pub(super) fn of_plan(plan: &OperationPlan, done: &[u32]) -> Self {
        Self::of(done, plan.steps.len())
    }

    /// Tells whether step `seq` is finished. Takes `&u32` like `<[u32]>::contains`, so call sites
    /// written for the slice keep their text.
    pub(super) fn contains(&self, seq: &u32) -> bool {
        self.bits
            .get(word_of(*seq))
            .is_some_and(|word| word & bit_of(*seq) != 0)
    }
}

/// Index of the 64-bit word that holds step `seq`.
fn word_of(seq: u32) -> usize {
    // u32 / 64 is below 2^26, so it fits every usize Rust supports (16-bit targets aside).
    (seq / 64) as usize
}

/// Mask of step `seq`'s bit inside its word.
fn bit_of(seq: u32) -> u64 {
    1 << (seq % 64)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::DoneSet;

    #[test]
    fn contains_exactly_the_given_steps_across_word_edges() {
        let set = DoneSet::of(&[0, 5, 64, 100_000], 100_001);
        for present in [0, 5, 64, 100_000] {
            assert!(set.contains(&present), "step {present} was given as done");
        }
        for absent in [1, 4, 6, 63, 65, 127, 128, 99_999, 100_001, u32::MAX] {
            assert!(!set.contains(&absent), "step {absent} was never done");
        }
    }

    #[test]
    fn empty_journal_has_no_done_step() {
        let set = DoneSet::of(&[], 10);
        assert!(!set.contains(&0), "an empty journal finished nothing");
    }

    #[test]
    fn corrupt_seq_past_the_plan_cannot_size_the_set() {
        let done = [3, u32::MAX, 9, 10];
        let set = DoneSet::of(&done, 10);
        assert!(
            set.bits.len() <= 1,
            "a 10-step plan needs one word, got {}",
            set.bits.len()
        );
        for seq in 0..10 {
            assert_eq!(set.contains(&seq), done.contains(&seq), "plan step {seq}");
        }
    }

    proptest! {
        /// Oracle: for every plan step the bitmap answers like the slice it replaced.
        #[test]
        fn answers_like_the_slice(
            steps in 0_usize..5_000,
            done in prop::collection::vec(prop_oneof![9 => 0_u32..6_000, 1 => any::<u32>()], 0..200),
        ) {
            let set = DoneSet::of(&done, steps);
            prop_assert!(set.bits.len() <= steps.div_ceil(64), "{} words", set.bits.len());
            for seq in (0_u32..).take(steps) {
                prop_assert_eq!(set.contains(&seq), done.contains(&seq), "plan step {}", seq);
            }
        }
    }
}
