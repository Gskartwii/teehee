use super::byte_rope::{Rope, RopeDelta};

struct Action {
    delta: RopeDelta,
}

impl Action {
    fn invert(&self, base_rope: &Rope) -> Action {
        let (inserts, deletions) = self.delta.clone().factor();
        let ins_subset = inserts.inserted_subset();
        let deleted_now = base_rope.without_subset(deletions.complement());
        dbg!(deleted_now.len());

        let deletions_from_base = dbg!(deletions.transform_expand(&ins_subset));
        let deletions_from_inverted = dbg!(ins_subset);
        Action {
            delta: RopeDelta::synthesize(
                &deleted_now.into_node(),
                &deletions_from_base,
                &deletions_from_inverted,
            ),
        }
    }
}

struct History {
    current_incomplete: Vec<Action>,

    undo: Vec<Action>,
    redo: Vec<Action>,
}

impl History {
    fn commit(&mut self) {}
}

#[cfg(test)]
mod test {
    use super::*;
    use xi_rope::delta::{Delta, DeltaElement};
    use xi_rope::DeltaBuilder;

    #[test]
    fn test_delete() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.delete(0..1);
        let deletion = delta_builder.build();
        let inversion = Action {
            delta: deletion.clone(),
        }
        .invert(&base_rope);

        let erased_rope = base_rope.apply_delta(&deletion);
        assert_eq!(&erased_rope.slice_to_cow(..), &vec![1, 2, 3]);
        let unerased_rope = erased_rope.apply_delta(&inversion.delta);
        assert_eq!(&unerased_rope.slice_to_cow(..), &vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_middle_delete() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.delete(1..3);
        let deletion = delta_builder.build();
        let inversion = Action {
            delta: deletion.clone(),
        }
        .invert(&base_rope);

        let erased_rope = base_rope.apply_delta(&deletion);
        assert_eq!(&erased_rope.slice_to_cow(..), &vec![0, 3]);
        let unerased_rope = erased_rope.apply_delta(&inversion.delta);
        assert_eq!(&unerased_rope.slice_to_cow(..), &vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_insert() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.replace(1..1, Into::<Rope>::into(vec![5]).into_node());
        let insertion = delta_builder.build();
        let inversion = Action {
            delta: insertion.clone(),
        }
        .invert(&base_rope);

        let inserted_rope = base_rope.apply_delta(&insertion);
        assert_eq!(&inserted_rope.slice_to_cow(..), &vec![0, 5, 1, 2, 3]);
        let uninserted_rope = inserted_rope.apply_delta(&inversion.delta);
        assert_eq!(&uninserted_rope.slice_to_cow(..), &vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_replace() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.replace(1..2, Into::<Rope>::into(vec![5, 6]).into_node());
        let sub = delta_builder.build();
        let inversion = Action { delta: sub.clone() }.invert(&base_rope);

        let replaced_rope = base_rope.apply_delta(&sub);
        assert_eq!(&replaced_rope.slice_to_cow(..), &vec![0, 5, 6, 2, 3]);
        let unreplaced_rope = replaced_rope.apply_delta(&inversion.delta);
        assert_eq!(&unreplaced_rope.slice_to_cow(..), &vec![0, 1, 2, 3]);
    }
}
