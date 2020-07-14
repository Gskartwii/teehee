use super::byte_rope::*;
use super::selection::*;
use xi_rope::{DeltaBuilder, Interval};

pub fn deletion(base: &Rope, selection: &Selection) -> RopeDelta {
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        let iv = Interval::new(region.min(), region.max() + 1);
        if !iv.is_empty() {
            builder.delete(iv);
        }
    }

    builder.build()
}

pub fn insert_before(base: &Rope, selection: &Selection, text: impl Into<Rope>) -> RopeDelta {
    let inserted = text.into();
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        let iv = Interval::new(region.min(), region.min());
        builder.replace(iv, inserted.clone().into_node());
    }

    builder.build()
}
