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
