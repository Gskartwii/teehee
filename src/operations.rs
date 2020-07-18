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

pub fn insert(base: &Rope, selection: &Selection, text: impl Into<Rope>) -> RopeDelta {
    let inserted = text.into();
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        let iv = Interval::new(region.caret, region.caret);
        builder.replace(iv, inserted.clone().into_node());
    }

    builder.build()
}

pub fn paste(
    base: &Rope,
    selection: &Selection,
    register_contents: &[impl Into<Rope> + Clone],
    after: bool,
) -> RopeDelta {
    let mut builder = DeltaBuilder::new(base.len());
    let last_value = register_contents.last().unwrap();
    let reg_iter = register_contents
        .into_iter()
        .chain(std::iter::repeat(last_value));
    for (region, pasted) in selection.iter().zip(reg_iter) {
        let insert_pos = if after {
            std::cmp::min(base.len(), region.max() + 1)
        } else {
            region.min()
        };
        let iv = Interval::new(insert_pos, insert_pos);
        builder.replace(iv, pasted.to_owned().into().into_node());
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

pub fn replace(base: &Rope, selection: &Selection, ch: u8) -> RopeDelta {
    let mut builder = DeltaBuilder::new(base.len());
    for region in selection.iter() {
        let iv = Interval::new(region.min(), region.max() + 1);
        builder.replace(
            iv,
            Rope::from(vec![ch; region.max() - region.min() + 1]).into_node(),
        );
    }

    builder.build()
}
