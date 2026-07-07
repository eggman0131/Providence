//! Terrain-owned immovable features — rock and trees (ADR 0017 §5, issue #7
//! Phase 3).
//!
//! The [ADR 0017](../../../docs/decisions/0017-vertex-heightfield-terrain.md) §5
//! seam: some world contents are flagged *not movable* by terrain shaping. This
//! module holds the **terrain-owned** ones (rock, trees) as a grid parallel to
//! the height field; worldgen places them ([`super::worldgen::place_features`])
//! and the shaping ops consult them so a cascade never silently destroys one
//! ([`super::shape`] refuses an op whose cascade would move an immovable).
//!
//! *Cross-subsystem* immovables (opponent buildings) and settlement placement
//! live above terrain and stay parked (ADR 0021 §5); this is terrain only.

use alloc::vec::Vec;

/// A terrain-owned immovable occupying a vertex. All kinds are immovable to
/// raise/lower today; *which* kind matters for content (placement rules) and,
/// later, rendering. Per-action immovability (ADR 0017 §5) is a future refinement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Rock — scattered on mountains.
    Rock,
    /// A tree — scattered on land.
    Tree,
}

/// Which vertices carry a terrain-owned immovable, parallel to the height field
/// (ADR 0017 §5). Row-major like [`super::HeightField`]: the vertex at `(x, y)`
/// is `y * width + x`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureMap {
    width: u32,
    height: u32,
    /// Row-major features; `None` where a vertex is bare. `cells.len() ==
    /// width * height`, upheld by every constructor.
    cells: Vec<Option<Feature>>,
}

impl FeatureMap {
    /// An all-bare map of `width × height` vertices — the "no immovables" state
    /// pure-height tests and shaping without a world use.
    #[must_use]
    pub fn empty(width: u32, height: u32) -> Self {
        let count = width as usize * height as usize;
        Self {
            width,
            height,
            cells: alloc::vec::from_elem(None, count),
        }
    }

    /// A map from an explicit row-major buffer, or `None` unless both dimensions
    /// are non-zero and `cells.len()` equals `width × height` (so a map can
    /// never be ragged). Worldgen builds one this way.
    #[must_use]
    pub fn from_cells(width: u32, height: u32, cells: Vec<Option<Feature>>) -> Option<Self> {
        if width == 0 || height == 0 || cells.len() != width as usize * height as usize {
            return None;
        }
        Some(Self {
            width,
            height,
            cells,
        })
    }

    /// Width in vertices.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height (depth) in vertices.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The feature at `(x, y)`, or `None` if the vertex is bare **or** out of
    /// bounds.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Feature> {
        if x < self.width && y < self.height {
            self.cells[y as usize * self.width as usize + x as usize]
        } else {
            None
        }
    }

    /// Whether `(x, y)` carries an immovable — the query the shaping cascade
    /// makes before moving a vertex. Out-of-bounds is not immovable.
    #[must_use]
    pub fn is_immovable(&self, x: u32, y: u32) -> bool {
        self.get(x, y).is_some()
    }

    /// How many vertices carry a feature — the census figure.
    #[must_use]
    pub fn count(&self) -> u32 {
        // A map holds at most width × height features, which fits u32.
        u32::try_from(self.cells.iter().filter(|cell| cell.is_some()).count()).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::{Feature, FeatureMap};
    use alloc::vec;

    #[test]
    fn an_empty_map_has_no_immovables() {
        let map = FeatureMap::empty(4, 3);
        assert_eq!((map.width(), map.height()), (4, 3));
        assert_eq!(map.count(), 0);
        assert!(!map.is_immovable(2, 1), "a bare vertex is movable");
        assert_eq!(map.get(2, 1), None);
    }

    #[test]
    fn from_cells_places_features_row_major() {
        let cells = vec![Some(Feature::Tree), None, None, Some(Feature::Rock)];
        let map = FeatureMap::from_cells(2, 2, cells).expect("a 2×2 buffer must build");
        assert_eq!(map.get(0, 0), Some(Feature::Tree));
        assert_eq!(map.get(1, 1), Some(Feature::Rock));
        assert!(map.is_immovable(0, 0));
        assert!(!map.is_immovable(1, 0));
        assert_eq!(map.count(), 2);
    }

    #[test]
    fn from_cells_rejects_a_mismatched_buffer() {
        assert!(FeatureMap::from_cells(2, 2, vec![None, None, None]).is_none());
        assert!(FeatureMap::from_cells(0, 2, vec![]).is_none());
    }

    #[test]
    fn out_of_bounds_is_bare_and_movable() {
        let map = FeatureMap::from_cells(1, 1, vec![Some(Feature::Rock)]).unwrap();
        assert_eq!(map.get(1, 0), None, "off the grid reads bare");
        assert!(!map.is_immovable(0, 1), "off the grid is not immovable");
    }
}
