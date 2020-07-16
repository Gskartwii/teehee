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

pub fn backspace(base: &Rope, selection: &Selection) -> RopeDelta {
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        if region.caret == 0 {
            continue;
        }
        let iv = Interval::new(region.caret - 1, region.caret);
        builder.delete(iv);
    }

    builder.build()
}

pub fn delete_cursor(base: &Rope, selection: &Selection) -> RopeDelta {
    let base_len = base.len();
    let mut builder = DeltaBuilder::new(base_len);
    for region in selection.iter() {
        let iv = Interval::new(region.caret, std::cmp::min(base_len, region.caret + 1));
        if !iv.is_empty() {
            builder.delete(iv);
        }
    }

    builder.build()
}

pub fn insert(
    base: &Rope,
    selection: &Selection,
    text: impl Into<Rope>,
    before: bool,
) -> RopeDelta {
    let inserted = text.into();
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        let fixed_caret = if before {
            region.caret
        } else {
            region.caret + 1
        };

        let iv = Interval::new(fixed_caret, fixed_caret);
        builder.replace(iv, inserted.clone().into_node());
    }

    builder.build()
}

pub fn change(base: &Rope, selection: &Selection, text: impl Into<Rope>) -> RopeDelta {
    let inserted = text.into();
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        let iv = Interval::new(region.caret, region.caret + 1);
        builder.replace(iv, inserted.clone().into_node());
    }

    builder.build()
}
