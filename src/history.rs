use super::byte_rope::{Rope, RopeDelta};
use xi_rope::delta::DeltaElement;

struct Action {
    delta: RopeDelta,
}

fn print_delta(delta: &RopeDelta) {
    println!("len: {}", delta.base_len);
    for el in &delta.els {
        match el {
            DeltaElement::Copy(start, end) => println!("\tcopy {}..{}", start, end),
            DeltaElement::Insert(n) => println!("\tinsert {}", n.len()),
        }
    }
}

impl Action {
    fn from_delta(delta: RopeDelta) -> Action {
        Action { delta }
    }
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
    fn chain(self, after_self: &Rope, next: RopeDelta) -> Action {
        let after_next = after_self.apply_delta(&next);
        let (ins1, del1) = self.delta.factor();
        let (ins2, del2) = next.factor();

        let inserted_first = ins1.inserted_subset();
        let del1_expanded = del1.transform_expand(&inserted_first);
        let inserted_in_mid_text = inserted_first.transform_shrink(&del1_expanded);
        let inserted_second = ins2.inserted_subset();
        let inserted_total = inserted_in_mid_text.transform_union(&inserted_second);
        let del2_expanded = del2.transform_expand(&inserted_second);
        let inserted_in_next_text = inserted_total.transform_shrink(&del2_expanded);

        let tombstones = after_next.without_subset(inserted_in_next_text.complement());

        let ins2_in_final_union = ins2
            .transform_expand(&del1_expanded, true)
            .inserted_subset();
        /*let ins1_in_final_union = ins1
            .transform_shrink(&del1_expanded)
            .transform_expand(&inserted_second, false)
            .inserted_subset();
        let insertions_in_final_union = ins1_in_final_union.union(&ins2_in_final_union);*/
        let insertions_in_final_union = dbg!(ins1
            .transform_shrink(&del1_expanded)
            .transform_expand(dbg!(&inserted_second), false)
            .inserted_subset());
        let deletions_from_final = del2_expanded
            .transform_union(&del1_expanded)
            .transform_expand(&insertions_in_final_union);
        Action {
            delta: RopeDelta::synthesize(
                &tombstones.into_node(),
                dbg!(&insertions_in_final_union),
                dbg!(&deletions_from_final),
            ),
        }
    }
}

struct History {
    current_incomplete: Option<Action>,

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

    #[test]
    fn test_chain_delete() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.delete(0..1);
        let deletion1 = delta_builder.build();
        let mid_rope = base_rope.apply_delta(&deletion1);

        let mut delta_builder2 = DeltaBuilder::new(mid_rope.len());
        delta_builder2.delete(0..1);
        let deletion2 = delta_builder2.build();
        let final_rope = mid_rope.apply_delta(&deletion2);

        assert_eq!(&final_rope.slice_to_cow(..), &vec![2, 3]);
        let chained_delta = Action::from_delta(deletion1).chain(&mid_rope, deletion2);
        let chain_final_rope = base_rope.apply_delta(&chained_delta.delta);
        assert_eq!(&chain_final_rope.slice_to_cow(..), &vec![2, 3]);
    }

    #[test]
    fn test_chain_delete_insert() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.delete(0..1);
        let deletion1 = delta_builder.build();
        let mid_rope = base_rope.apply_delta(&deletion1);

        let mut delta_builder2 = DeltaBuilder::new(mid_rope.len());
        delta_builder2.replace(1..1, Into::<Rope>::into(vec![5, 6]).into_node());
        let insertion = delta_builder2.build();
        let final_rope = mid_rope.apply_delta(&insertion);

        assert_eq!(&final_rope.slice_to_cow(..), &vec![1, 5, 6, 2, 3]);
        let chained_delta = Action::from_delta(deletion1).chain(&mid_rope, insertion);
        let chain_final_rope = base_rope.apply_delta(&chained_delta.delta);
        assert_eq!(&chain_final_rope.slice_to_cow(..), &vec![1, 5, 6, 2, 3]);
    }

    #[test]
    fn test_chain_insert() {
        let base_rope: Rope = vec![0, 1, 2, 3].into();
        let mut delta_builder = DeltaBuilder::new(base_rope.len());
        delta_builder.replace(1..1, Into::<Rope>::into(vec![5]).into_node());
        let insertion1 = delta_builder.build();
        let mid_rope = base_rope.apply_delta(&insertion1);

        let mut delta_builder2 = DeltaBuilder::new(mid_rope.len());
        delta_builder2.replace(2..2, Into::<Rope>::into(vec![6]).into_node());
        let insertion2 = delta_builder2.build();
        let final_rope = mid_rope.apply_delta(&insertion2);

        assert_eq!(&final_rope.slice_to_cow(..), &vec![0, 5, 6, 1, 2, 3]);
        let chained_delta = Action::from_delta(insertion1).chain(&mid_rope, insertion2);
        let chain_final_rope = base_rope.apply_delta(&chained_delta.delta);
        assert_eq!(&chain_final_rope.slice_to_cow(..), &vec![0, 1, 5, 6, 2, 3]);
    }
}
