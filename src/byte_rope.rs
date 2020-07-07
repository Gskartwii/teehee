use std::borrow::Cow;
use std::convert::From;
use std::fmt;
use xi_rope::delta::*;
use xi_rope::interval::*;
use xi_rope::tree::*;

const MIN_LEAF: usize = 511;
const MAX_LEAF: usize = 1024;

#[derive(Debug, PartialEq, Eq, Clone, Hash, Default)]
pub struct Bytes(pub Vec<u8>);
pub struct Rope(Node<RopeInfo>);
pub type RopeDelta = Delta<RopeInfo>;
pub type RopeDeltaElement = DeltaElement<RopeInfo>;

impl Leaf for Bytes {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_ok_child(&self) -> bool {
        self.0.len() >= MIN_LEAF
    }
    fn push_maybe_split(&mut self, other: &Bytes, iv: Interval) -> Option<Bytes> {
        let (start, end) = iv.start_end();
        self.0.extend_from_slice(&other.0[start..end]);
        if self.0.len() <= MAX_LEAF {
            None
        } else {
            let split_point = MAX_LEAF;
            let right_bytes = self.0.split_off(split_point);
            self.0.shrink_to_fit();
            Some(Bytes(right_bytes))
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct RopeInfo();

impl NodeInfo for RopeInfo {
    type L = Bytes;

    fn accumulate(&mut self, _: &Self) {}
    fn compute_info(_: &Bytes) -> Self {
        Default::default()
    }
}

impl DefaultMetric for RopeInfo {
    type DefaultMetric = BaseMetric;
}

#[derive(Clone, Copy)]
pub struct BaseMetric();

impl Metric<RopeInfo> for BaseMetric {
    fn measure(_: &RopeInfo, len: usize) -> usize {
        len
    }

    fn to_base_units(_: &Bytes, in_measured_units: usize) -> usize {
        in_measured_units
    }

    fn from_base_units(_: &Bytes, in_base_units: usize) -> usize {
        in_base_units
    }

    fn is_boundary(_: &Bytes, _: usize) -> bool {
        true
    }

    fn prev(_: &Bytes, offset: usize) -> Option<usize> {
        match offset {
            0 => None,
            o => Some(o - 1),
        }
    }
    fn next(b: &Bytes, offset: usize) -> Option<usize> {
        if offset == b.len() {
            None
        } else {
            Some(offset + 1)
        }
    }

    fn can_fragment() -> bool {
        false
    }
}

impl Rope {
    pub fn len(&self) -> usize {
		self.0.len()
    }

    pub fn is_empty(&self) -> bool {
		self.0.len() == 0
    }

    pub fn iter_chunks<T: IntervalBounds>(&self, range: T) -> ChunkIter {
        let Interval { start, end } = range.into_interval(self.0.len());
        ChunkIter {
            cursor: Cursor::new(&self.0, start),
            end,
        }
    }

    pub fn slice_to_cow<T: IntervalBounds>(&self, range: T) -> Cow<[u8]> {
		let mut iter = self.iter_chunks(range);
		let first = iter.next();
		let second = iter.next();

		match (first, second) {
    		(None, None) => Cow::from(vec![]),
    		(Some(b), None) => Cow::from(b),
    		(Some(one), Some(two)) => {
        		let mut result = [one, two].concat();
        		for chunk in iter {
            		result.extend_from_slice(chunk);
        		}
        		Cow::from(result)
    		}
    		_ => unreachable!(),
		}
    }
}

impl From<Vec<u8>> for Rope {
    fn from(mut vec: Vec<u8>) -> Self {
        let mut builder = TreeBuilder::new();
        if vec.len() <= MAX_LEAF {
            if !vec.is_empty() {
                builder.push_leaf(Bytes(vec));
            }
            return Rope(builder.build());
        }
        while !vec.is_empty() {
            let split_point = std::cmp::min(vec.len(), MAX_LEAF);
            let rest = vec.split_off(split_point);
            builder.push_leaf(Bytes(vec));
            vec = rest;
        }
        Rope(builder.build())
    }
}

impl From<Rope> for Vec<u8> {
    fn from(rope: Rope) -> Self {
        Vec::from(&rope)
    }
}

impl<'a> From<&'a Rope> for Vec<u8> {
    fn from(rope: &Rope) -> Self {
        rope.iter_chunks(..).fold(vec![], |mut acc, x| {
            acc.extend_from_slice(x);
            acc
        })
    }
}

pub struct ChunkIter<'a> {
    cursor: Cursor<'a, RopeInfo>,
    end: usize,
}
impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.cursor.pos() >= self.end {
            return None;
        }
        let (leaf, start_pos) = self.cursor.get_leaf().unwrap();
        let len = std::cmp::min(self.end - self.cursor.pos(), leaf.len() - start_pos);
        self.cursor.next_leaf();
        Some(&leaf.0[start_pos..start_pos + len])
    }
}

impl fmt::Display for Rope {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for byte in self.iter_chunks(..).flatten() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}
impl fmt::Debug for Rope {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !f.alternate() {
            write!(f, "Rope(")?;
        }
        for byte in self.iter_chunks(..).flatten() {
            write!(f, "{:02x}", byte)?;
        }
        if !f.alternate() {
            write!(f, ")")?;
        }
        Ok(())
    }
}
