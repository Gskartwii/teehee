use super::byte_rope::RopeDelta;

use std::cmp;
use std::default::Default;
use xi_rope::{Interval, Transformer};

#[derive(Debug, PartialEq, Clone)]
pub struct Selection {
    // INVARIANT: regions should be sorted by starting points
    // INVARIANT: regions should not overlap
    regions: Vec<SelRegion>,
    pub main_selection: usize,
}

impl Default for Selection {
    fn default() -> Selection {
        let mut sel = Selection {
            regions: vec![SelRegion::new(0, 0)],
            main_selection: 0,
        };
        sel.regions[0].main = true;
        sel
    }
}

impl Selection {
    pub fn new() -> Selection {
        Default::default()
    }

    pub fn clear(&mut self) {
        self.regions = vec![Default::default()];
        self.regions[0].main = true;
        self.main_selection = 0;
    }

    pub fn len_bytes(&self) -> usize {
        self.regions.iter().map(SelRegion::len).sum()
    }

    pub fn retain(&mut self, index: usize) {
        let mut main = self.regions[index];
        self.main_selection = 0;
        main.main = true;
        self.regions = vec![main];
    }

    pub fn remove(&mut self, index: usize) {
        if self.regions.len() == 1 {
            return;
        }
        self.regions.remove(index);
        self.main_selection = std::cmp::min(self.regions.len() - 1, self.main_selection);
        self.regions[self.main_selection].main = true;
    }

    pub fn select_all(&mut self, buf_size: usize) {
        self.clear();
        if buf_size == 0 {
            return;
        }
        self.regions[0].tail = 0;
        self.regions[0].caret = buf_size - 1;
    }

    pub fn main(&self) -> SelRegion {
        self.regions[self.main_selection]
    }

    fn search(&self, offset: usize) -> usize {
        if offset > self.regions.last().unwrap().max() {
            return self.regions.len();
        }
        self.regions
            .binary_search_by(|r| r.max().cmp(&offset))
            .unwrap_or_else(std::convert::identity)
    }

    pub fn regions_in_range(&self, start: usize, end: usize) -> &[SelRegion] {
        let first = self.search(start);
        let mut last = self.search(end);
        if last < self.regions.len() && self.regions[last].min() <= end {
            last += 1;
        }
        &self.regions[first..last]
    }

    pub fn apply_delta(&mut self, delta: &RopeDelta, max_len: usize) {
        let new_max_len = delta.new_document_len();
        if new_max_len == 0 {
            self.clear();
            return;
        }

        let mut transformer = Transformer::new(delta);
        self.map_selections(|region| {
            let new_region = SelRegion::new(
                if max_len == region.caret {
                    new_max_len
                } else {
                    std::cmp::min(new_max_len, transformer.transform(region.caret, true))
                },
                if max_len == region.tail {
                    new_max_len
                } else {
                    std::cmp::min(new_max_len, transformer.transform(region.tail, true))
                },
            );
            vec![new_region]
        })
    }

    pub fn apply_delta_offset_carets(
        &mut self,
        delta: &RopeDelta,
        caret_offset: isize,
        tail_offset: isize,
        max_len: usize,
    ) {
        let new_max_len = delta.new_document_len();
        if new_max_len == 0 {
            self.clear();
            return;
        }

        let mut transformer = Transformer::new(delta);
        self.map_selections(|region| {
            let new_region = SelRegion::new(
                if max_len == region.caret {
                    (new_max_len as isize + caret_offset) as usize
                } else {
                    std::cmp::min(
                        new_max_len,
                        (transformer.transform(region.caret, true) as isize + caret_offset)
                            as usize,
                    )
                },
                if max_len == region.tail {
                    (new_max_len as isize + tail_offset) as usize
                } else {
                    std::cmp::min(
                        new_max_len,
                        (transformer.transform(region.tail, true) as isize + tail_offset) as usize,
                    )
                },
            );
            vec![new_region]
        })
    }

    pub fn map_selections(&mut self, mut f: impl FnMut(SelRegion) -> Vec<SelRegion>) {
        let mut regions_out: Vec<SelRegion> = vec![];
        let mut new_main_sel = 0;
        for (i, region) in self.regions.iter().copied().enumerate() {
            for new_region in f(region) {
                if regions_out.is_empty() || !regions_out.last().unwrap().overlaps(&new_region) {
                    regions_out.push(new_region);
                } else if let Some(last) = regions_out.pop() {
                    regions_out.push(last.merge(&new_region));
                }
            }
            if i == self.main_selection {
                new_main_sel = regions_out.len() - 1;
                regions_out.last_mut().unwrap().main = true;
            }
        }
        self.regions = regions_out;
        self.main_selection = new_main_sel;
    }

    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn main_cursor_offset(&self) -> usize {
        self.regions[self.main_selection].caret
    }

    pub fn iter(&self) -> impl Iterator<Item = &SelRegion> {
        self.regions.iter()
    }

    pub fn select_next(&mut self, count: usize) {
        self.regions[self.main_selection].main = false;
        self.main_selection = (self.main_selection + count) % self.regions.len();
        self.regions[self.main_selection].main = true;
    }

    pub fn select_prev(&mut self, count: usize) {
        self.regions[self.main_selection].main = false;
        self.main_selection = (self.main_selection + self.regions.len()
            - count % self.regions.len())
            % self.regions.len();
        self.regions[self.main_selection].main = true;
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SelRegion {
    // Start of selection, inclusive
    pub caret: usize,
    // End of selection, exclusive
    pub tail: usize,

    main: bool,
}

impl Default for SelRegion {
    fn default() -> SelRegion {
        SelRegion::new(0, 0)
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl SelRegion {
    pub fn new(caret: usize, tail: usize) -> Self {
        SelRegion {
            caret,
            tail,
            main: false,
        }
    }

    pub fn is_main(&self) -> bool {
        self.main
    }

    pub fn with_direction(self, backward: bool) -> SelRegion {
        let max = cmp::max(self.caret, self.tail);
        let min = cmp::min(self.caret, self.tail);
        if backward {
            SelRegion::new(min, max)
        } else {
            SelRegion::new(max, min)
        }
    }

    pub fn max(&self) -> usize {
        cmp::max(self.caret, self.tail)
    }

    pub fn min(&self) -> usize {
        cmp::min(self.caret, self.tail)
    }

    pub fn len(&self) -> usize {
        self.max() - self.min() + 1
    }

    pub fn overlaps(&self, other: &SelRegion) -> bool {
        self.max() >= other.min()
    }

    pub fn simple_move(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
        count: usize,
    ) -> SelRegion {
        if max_size == 0 {
            return *self;
        }

        let old_caret = self.caret;
        let caret_location = match direction {
            Direction::Up => cmp::max(
                0,
                old_caret as isize - (bytes_per_line as isize).saturating_mul(count as isize),
            ) as usize,
            Direction::Down => cmp::min(max_size, old_caret + bytes_per_line.saturating_mul(count)),
            Direction::Left => cmp::max(0, old_caret as isize - count as isize) as usize,
            Direction::Right => cmp::min(max_size, old_caret + count),
        };

        SelRegion::new(caret_location, caret_location)
    }

    pub fn simple_extend(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
        count: usize,
    ) -> SelRegion {
        if max_size == 0 {
            return *self;
        }

        let old_caret = self.caret;
        let caret_location = match direction {
            Direction::Up => cmp::max(
                0,
                old_caret as isize - (bytes_per_line as isize).saturating_mul(count as isize),
            ) as usize,
            Direction::Down => cmp::min(max_size, old_caret + bytes_per_line.saturating_mul(count)),
            Direction::Left => cmp::max(0, old_caret as isize - count as isize) as usize,
            Direction::Right => cmp::min(max_size, old_caret + count),
        };
        SelRegion::new(caret_location, self.tail)
    }

    pub fn jump_to(&self, offset: usize) -> SelRegion {
        SelRegion::new(offset, offset)
    }

    pub fn extend_to(&self, offset: usize) -> SelRegion {
        SelRegion::new(offset, self.tail)
    }

    pub fn jump_to_boundary(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
    ) -> SelRegion {
        if max_size == 0 {
            return *self;
        }

        let caret_location = match direction {
            Direction::Up => 0,
            Direction::Down => max_size - 1, // Don't do overflow selection in jumps
            Direction::Left => self.caret - (self.caret % bytes_per_line),
            Direction::Right => std::cmp::min(
                self.caret + bytes_per_line - (self.caret % bytes_per_line) - 1,
                max_size - 1,
            ),
        };
        SelRegion::new(caret_location, caret_location)
    }

    pub fn extend_to_boundary(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
    ) -> SelRegion {
        if max_size == 0 {
            return *self;
        }

        let caret_location = match direction {
            Direction::Up => 0,
            Direction::Down => max_size - 1, // Don't do overflow selection in jumps
            Direction::Left => self.caret - (self.caret % bytes_per_line),
            Direction::Right => std::cmp::min(
                self.caret + bytes_per_line - (self.caret % bytes_per_line) - 1,
                max_size - 1,
            ),
        };
        SelRegion::new(caret_location, self.tail)
    }

    pub fn swap_caret(&self) -> SelRegion {
        SelRegion::new(self.tail, self.caret)
    }

    pub fn collapse(&self) -> SelRegion {
        SelRegion::new(self.caret, self.caret)
    }

    pub fn forward(&self) -> bool {
        self.caret >= self.tail
    }

    pub fn backward(&self) -> bool {
        self.caret <= self.tail
    }

    pub fn to_backward(self) -> SelRegion {
        SelRegion::new(self.min(), self.max())
    }

    pub fn to_forward(self) -> SelRegion {
        SelRegion::new(self.max(), self.min())
    }

    pub fn merge(&self, other: &SelRegion) -> SelRegion {
        let both_forward = self.forward() && other.forward();
        let both_backward = self.backward() && other.backward();
        let mut merged = match (both_forward, both_backward) {
            (true, true) => {
                assert_eq!(
                    self.caret, other.caret,
                    "Can't merge disjoint cursor selections"
                );
                *self
            }
            (true, false) => SelRegion::new(
                cmp::max(self.caret, other.caret),
                cmp::min(self.tail, other.tail),
            ),
            (false, true) => SelRegion::new(
                cmp::min(self.caret, other.caret),
                cmp::max(self.tail, other.tail),
            ),
            _ => panic!("Can't merge selections going in different directions"),
        };
        if self.main || other.main {
            merged.main = true;
        }
        merged
    }

    pub fn inherit_direction(&self, parent: &SelRegion) -> SelRegion {
        if parent.forward() {
            self.to_forward()
        } else {
            self.to_backward()
        }
    }

    pub fn split_at_region(
        &self,
        start: usize,
        end: usize,
    ) -> (Option<SelRegion>, Option<SelRegion>) {
        if start <= self.min() {
            if end >= self.max() {
                return (None, None);
            }

            return (
                None,
                Some(SelRegion::new(end + 1, self.max()).inherit_direction(self)),
            );
        }
        if end >= self.max() {
            return (
                Some(SelRegion::new(self.min(), start - 1).inherit_direction(self)),
                None,
            );
        }
        (
            Some(SelRegion::new(self.min(), start - 1)),
            Some(SelRegion::new(end + 1, self.max())),
        )
    }
}

impl From<SelRegion> for Interval {
    fn from(sel_region: SelRegion) -> Self {
        (sel_region.min()..=sel_region.max()).into()
    }
}
