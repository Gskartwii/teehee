use xi_rope::Interval;

use std::collections::HashMap;

use super::byte_rope::*;
use super::selection::*;
use super::state::*;

pub struct Buffer {
    pub data: Rope,
    pub selection: Selection,
    pub registers: HashMap<char, Vec<Vec<u8>>>,
}

impl Buffer {
    pub fn from_data(data: Vec<u8>) -> Buffer {
        Buffer {
            data: data.into(),
            selection: Selection::new(),
            registers: HashMap::new(),
        }
    }

    pub fn map_selections(&mut self, mut f: impl FnMut(SelRegion) -> Vec<SelRegion>) -> DirtyBytes {
        let mut invalidated_ranges = Vec::new();
        self.selection.map_selections(|region| {
            invalidated_ranges.push(Interval::from(region.min()..=region.max()));
            let new = f(region);
            for new_reg in new.iter() {
                invalidated_ranges.push(Interval::from(new_reg.min()..=new_reg.max()));
            }
            new
        });
        invalidated_ranges.sort_by(|a, b| a.start.cmp(&b.start));

        let mut disjoint_invalidated_ranges = Vec::new();
        for r in invalidated_ranges {
            if disjoint_invalidated_ranges.is_empty() {
                disjoint_invalidated_ranges.push(r);
                continue;
            }
            let last = disjoint_invalidated_ranges.last().unwrap();
            if last.contains(r.start) {
                *disjoint_invalidated_ranges.last_mut().unwrap() = last.union(r);
                continue;
            }
            disjoint_invalidated_ranges.push(r);
        }
        DirtyBytes::ChangeInPlace(disjoint_invalidated_ranges)
    }

    pub fn apply_delta(&mut self, delta: &RopeDelta) -> DirtyBytes {
        self.selection.apply_delta(&delta);
        self.data = self.data.apply_delta(&delta);

        DirtyBytes::ChangeLength
    }

    pub fn apply_delta_offset_carets(
        &mut self,
        delta: &RopeDelta,
        caret_offset: isize,
        tail_offset: isize,
    ) -> DirtyBytes {
        self.selection
            .apply_delta_offset_carets(delta, caret_offset, tail_offset);
        self.data = self.data.apply_delta(&delta);

        DirtyBytes::ChangeLength
    }

    fn switch_main_sel(&mut self, f: impl FnOnce(&mut Selection)) -> DirtyBytes {
        let old_main_sel_interval = self.selection.main().into();
        f(&mut self.selection);
        let new_main_sel_interval = self.selection.main().into();
        DirtyBytes::ChangeInPlace(vec![old_main_sel_interval, new_main_sel_interval])
    }

    fn modify_sels_in_place(&mut self, f: impl FnOnce(&mut Selection)) -> DirtyBytes {
        let dirty =
            DirtyBytes::ChangeInPlace(self.selection.iter().copied().map(Into::into).collect());
        f(&mut self.selection);

        dirty
    }

    pub fn remove_main_sel(&mut self) -> DirtyBytes {
        self.switch_main_sel(Selection::remove_main)
    }
    pub fn retain_main_sel(&mut self) -> DirtyBytes {
        self.modify_sels_in_place(Selection::retain_main)
    }
    pub fn select_next(&mut self) -> DirtyBytes {
        self.switch_main_sel(Selection::select_next)
    }
    pub fn select_prev(&mut self) -> DirtyBytes {
        self.switch_main_sel(Selection::select_prev)
    }

    pub fn yank_selections(&mut self, reg: char) {
        if self.data.is_empty() {
            self.registers
                .insert(reg, vec![vec![]; self.selection.len()]);
            return;
        }

        let selections = self
            .selection
            .iter()
            .map(|region| self.data.slice_to_cow(region.min()..=region.max()).to_vec())
            .collect();
        self.registers.insert(reg, selections);
    }
}
