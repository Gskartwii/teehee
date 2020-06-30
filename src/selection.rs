use std::default::Default;
use xi_rope::{RopeDelta, Transformer};

#[derive(Debug, PartialEq, Clone)]
pub struct Selection {
    regions: Vec<SelRegion>,
    main_selection: usize,
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
        if offset > self.regions.last().unwrap().end {
            return self.regions.len();
        }
        self.regions
            .binary_search_by(|r| r.end.cmp(&offset))
            .unwrap_or_else(std::convert::identity)
    }

    pub fn regions_in_range(&self, start: usize, end: usize) -> &[SelRegion] {
        let first = self.search(start);
        let mut last = self.search(end);
        if last < self.regions.len() && self.regions[last].start <= end {
            last += 1;
        }
        &self.regions[first..last]
    }

    pub fn apply_delta(&self, delta: &RopeDelta) -> Selection {
        let mut result = Selection::new();
        let mut transformer = Transformer::new(delta);
        let mut new_main_sel = 0;

        for (i, region) in self.regions.iter().enumerate() {
            let mut new_region = SelRegion::new(
                transformer.transform(region.start, true),
                transformer.transform(region.end, true),
            );
            new_region.caret_pos = region.caret_pos;
            if result.regions.len() == 0 || !result.regions.last().unwrap().overlaps(&new_region) {
                result.regions.push(new_region);
            }
            if i == self.main_selection {
                new_main_sel = result.regions.len() - 1;
            }
        }
        result
    }

    pub fn map_selections(&mut self, f: impl FnMut(SelRegion) -> SelRegion) {
        self.regions = self.regions.iter().copied().map(f).collect();
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CaretPosition {
    Start,
    End,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SelRegion {
    // Start of selection, inclusive
    pub start: usize,
    // End of selection, exclusive
    pub end: usize,
    pub caret_pos: CaretPosition,
}

impl Default for SelRegion {
    fn default() -> SelRegion {
        SelRegion::new(0, 1)
    }
}

impl SelRegion {
    pub fn new(start: usize, end: usize) -> Self {
        SelRegion {
            start,
            end,
            caret_pos: CaretPosition::Start,
        }
    }

    pub fn caret(&self) -> usize {
        match self.caret_pos {
            CaretPosition::Start => self.start,
            CaretPosition::End => self.end,
        }
    }

    pub fn overlaps(&self, other: &SelRegion) -> bool {
        self.end > other.start
    }
}
