use super::byte_rope::{Rope, RopeDelta};

struct Action {
	delta: RopeDelta,
}

impl Action {
    fn invert(&self, base_rope: &Rope) -> Action {
        let (inserts, deletions) = self.delta.clone().factor();
    }
}

struct History {
    current_incomplete: Vec<Action>,

    undo: Vec<Action>,
    redo: Vec<Action>,
}

impl History {
    fn commit(&mut self) {
    }
}
