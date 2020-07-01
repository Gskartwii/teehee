use std::cmp;
use std::default::Default;
use xi_rope::{RopeDelta, Transformer};

#[derive(Debug, PartialEq, Clone)]
pub struct Selection {
    // INVARIANT: regions should be sorted by starting points
    // INVARIANT: regions should not overlap
    regions: Vec<SelRegion>,
    pub main_selection: usize,
}

impl Default for Selection {
    fn default() -> Selection {
        Selection {
            regions: vec![Default::default()],
            main_selection: 0,
        }
    }
}

impl Selection {
    pub fn new() -> Selection {
        Default::default()
    }

    pub fn clear(&mut self) {
        self.regions = vec![Default::default()];
        self.main_selection = 0;
    }

    pub fn retain_main(&mut self) {
        let main = self.regions[self.main_selection];
        self.main_selection = 0;
        self.regions = vec![main];
    }

    pub fn remove_main(&mut self) {
        self.regions.remove(self.main_selection);
        self.main_selection = std::cmp::min(self.regions.len() - 1, self.main_selection);
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

    pub fn apply_delta(&mut self, delta: &RopeDelta) {
        let mut transformer = Transformer::new(delta);
        self.map_selections(|region| {
            let new_region = SelRegion::new(
                transformer.transform(region.caret, true),
                transformer.transform(region.tail, true),
            );
            vec![new_region]
        })
    }

    pub fn map_selections(&mut self, mut f: impl FnMut(SelRegion) -> Vec<SelRegion>) {
        let mut regions_out: Vec<SelRegion> = vec![];
        let mut new_main_sel = 0;
        for (i, region) in self.regions.iter().copied().enumerate() {
            for new_region in f(region) {
                if regions_out.len() == 0 || !regions_out.last().unwrap().overlaps(&new_region) {
                    regions_out.push(new_region);
                } else if let Some(last) = regions_out.pop() {
                    regions_out.push(last.merge(&new_region));
                }
            }
            if i == self.main_selection {
                new_main_sel = regions_out.len() - 1;
            }
        }
        self.regions = regions_out;
        self.main_selection = new_main_sel;
    }

    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SelRegion {
    // Start of selection, inclusive
    pub caret: usize,
    // End of selection, exclusive
    pub tail: usize,
}

impl Default for SelRegion {
    fn default() -> SelRegion {
        SelRegion::new(0, 0)
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl SelRegion {
    pub fn new(caret: usize, tail: usize) -> Self {
        SelRegion { caret, tail }
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

    pub fn overlaps(&self, other: &SelRegion) -> bool {
        self.max() >= other.min()
    }

    pub fn simple_move(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
    ) -> SelRegion {
        let old_caret = self.caret;
        let caret_location = match direction {
            Direction::Up => {
                let new_pos = old_caret as isize - bytes_per_line as isize;
                let is_oob = new_pos < 0;
                if is_oob {
                    old_caret
                } else {
                    new_pos as usize
                }
            }
            Direction::Down => cmp::min(max_size - 1, old_caret + bytes_per_line),
            Direction::Left => cmp::max(0, old_caret as isize - 1) as usize,
            Direction::Right => cmp::min(max_size - 1, old_caret + 1),
        };
        SelRegion::new(caret_location, caret_location)
    }

    pub fn simple_extend(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
    ) -> SelRegion {
        let old_caret = self.caret;
        let caret_location = match direction {
            Direction::Up => {
                let new_pos = old_caret as isize - bytes_per_line as isize;
                let is_oob = new_pos < 0;
                if is_oob {
                    old_caret
                } else {
                    new_pos as usize
                }
            }
            Direction::Down => cmp::min(max_size - 1, old_caret + bytes_per_line),
            Direction::Left => cmp::max(0, old_caret as isize - 1) as usize,
            Direction::Right => cmp::min(max_size - 1, old_caret + 1),
        };
        SelRegion::new(caret_location, self.tail)
    }

    pub fn jump_to_boundary(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
    ) -> SelRegion {
		let caret_location = match direction {
    		Direction::Up => 0,
    		Direction::Down => max_size - 1,
    		Direction::Left => self.caret - (self.caret % bytes_per_line),
    		Direction::Right => self.caret + bytes_per_line - (self.caret % bytes_per_line) - 1,
		};
		SelRegion::new(caret_location, caret_location)
    }

    pub fn extend_to_boundary(
        &self,
        direction: Direction,
        bytes_per_line: usize,
        max_size: usize,
    ) -> SelRegion {
		let caret_location = match direction {
    		Direction::Up => 0,
    		Direction::Down => max_size - 1,
    		Direction::Left => self.caret - (self.caret % bytes_per_line),
    		Direction::Right => self.caret + bytes_per_line - (self.caret % bytes_per_line) - 1,
		};
		SelRegion::new(caret_location, self.tail)
    }

    pub fn forward(&self) -> bool {
        self.caret >= self.tail
    }

    pub fn backward(&self) -> bool {
        self.caret <= self.tail
    }

    pub fn merge(&self, other: &SelRegion) -> SelRegion {
        let both_forward = self.forward() && other.forward();
        let both_backward = self.backward() && other.backward();
        match (both_forward, both_backward) {
            (true, true) => {
                assert_eq!(self, other, "Can't merge disjoint cursor selections");
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
        }
    }
}
