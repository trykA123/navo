use super::layout::Anchor;

const DEPTH: usize = 50;

#[derive(Default)]
pub(super) struct Trail {
    back: Vec<Anchor>,
    forward: Vec<Anchor>,
}

fn push(stack: &mut Vec<Anchor>, spot: Anchor) {
    if stack.len() == DEPTH {
        stack.remove(0);
    }
    stack.push(spot);
}

impl Trail {
    pub(super) fn leave(&mut self, from: Anchor) {
        push(&mut self.back, from);
        self.forward.clear();
    }

    pub(super) fn step_back(&mut self, here: Anchor) -> Option<Anchor> {
        let to = self.back.pop()?;
        push(&mut self.forward, here);
        Some(to)
    }

    pub(super) fn step_forward(&mut self, here: Anchor) -> Option<Anchor> {
        let to = self.forward.pop()?;
        push(&mut self.back, here);
        Some(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(page: u32) -> Anchor {
        Anchor { page, fraction: 0. }
    }

    #[test]
    fn back_and_forward_walk_the_same_positions() {
        let mut trail = Trail::default();
        trail.leave(at(1));
        trail.leave(at(5));
        assert_eq!(trail.step_back(at(9)), Some(at(5)));
        assert_eq!(trail.step_back(at(5)), Some(at(1)));
        assert_eq!(trail.step_back(at(1)), None);
        assert_eq!(trail.step_forward(at(1)), Some(at(5)));
        assert_eq!(trail.step_forward(at(5)), Some(at(9)));
        assert_eq!(trail.step_forward(at(9)), None);
    }

    #[test]
    fn a_new_jump_clears_forward_and_the_stack_keeps_fifty() {
        let mut trail = Trail::default();
        for page in 0..60 {
            trail.leave(at(page));
        }
        assert_eq!(trail.back.len(), DEPTH);
        assert_eq!(trail.back[0], at(10));
        trail.step_back(at(60));
        trail.leave(at(70));
        assert_eq!(trail.step_forward(at(70)), None);
    }
}
