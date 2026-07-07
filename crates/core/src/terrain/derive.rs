//! Terrain-type and buildability **derivations** (ADR 0017 §1, issue #7
//! Phase 1).
//!
//! Height lives on vertices ([`super::field`]); *type* and *buildability* hold
//! no state of their own — they are **derived** from the integer heights and
//! the sea-level datum, so they can never drift out of step with the field.
//! These are pure readers: they classify, they never mutate (the only writers
//! are the shaping ops in [`super::shape`]).
//!
//! - A **vertex** takes a [`TerrainType`] from its height relative to sea level
//!   and the shore/mountain bands ([`classify_vertex`]).
//! - A **face** — the square between four vertices — is **buildable** when it is
//!   flat and dry ([`is_buildable_face`]); *contiguity*, the third clause of the
//!   ADR 0017 §1 buildable definition, is a settlement-placement concern and
//!   arrives with that (parked) subsystem.
//!
//! The thresholds are **parameters, not literals** (I1): the sea-level datum is
//! `sim.worldgen.sea_level` and the shore/mountain bands are `content.terrain.*`
//! ([ADR 0021](../../../docs/decisions/0021-seeded-parameterised-worldgen.md);
//! the keys land with worldgen, Phase 2). They reach these functions as
//! arguments, so the module stays free of the config types **and** worldgen can
//! classify a *candidate* height before it is ever written into a field — the
//! same code that counts how much land a seed yields.

use super::{Height, HeightField};

/// A vertex's derived terrain type (ADR 0017 §1). Carries no state — recomputed
/// from height and sea level on demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainType {
    /// At or below the sea-level datum — underwater.
    Water,
    /// Dry land within the shore band just above sea level — the coastline.
    Shore,
    /// Ordinary dry land between shore and mountain.
    Land,
    /// High ground at or above the mountain threshold.
    Mountain,
}

/// Classify a vertex height into a [`TerrainType`] (ADR 0017 §1).
///
/// Pure and total: it reads only the height and the thresholds, so worldgen can
/// call it on a *candidate* height (to count how much land a seed yields)
/// exactly as a derivation over a stored field can. Precedence is
/// **water → mountain → shore → land**, so a submerged or high vertex is never
/// mislabelled a shore even under a degenerately wide `shore_band`.
///
/// - `sea_level` — the waterline datum (`sim.worldgen.sea_level`); heights at or
///   below it are [`TerrainType::Water`].
/// - `shore_band` — how many height steps above sea level still count as
///   [`TerrainType::Shore`] (`content.terrain.shore.band`); `0` means no shore.
/// - `mountain_min` — the height at or above which a vertex is
///   [`TerrainType::Mountain`] (`content.terrain.mountain.min_height`).
#[must_use]
pub fn classify_vertex(
    height: Height,
    sea_level: Height,
    shore_band: u32,
    mountain_min: Height,
) -> TerrainType {
    if height <= sea_level {
        TerrainType::Water
    } else if height >= mountain_min {
        TerrainType::Mountain
    } else if height <= sea_level.saturating_add_unsigned(shore_band) {
        TerrainType::Shore
    } else {
        TerrainType::Land
    }
}

/// Whether the face whose top-left corner is `(fx, fy)` is **buildable**: flat
/// (its four corner vertices share one height) and dry (that height is above
/// `sea_level`). ADR 0017 §1.
///
/// Returns `None` when the face is out of range — a face needs its right and
/// lower neighbours, so valid faces have `fx` in `0..width-1` and `fy` in
/// `0..height-1`. The third buildable clause, *contiguity* with neighbouring
/// buildable faces, is a settlement-placement property and lands with that
/// (parked) subsystem; a single face's flat-and-dry test is local and lives
/// here.
#[must_use]
pub fn is_buildable_face(field: &HeightField, fx: u32, fy: u32, sea_level: Height) -> Option<bool> {
    let top_left = field.get(fx, fy)?;
    let top_right = field.get(fx + 1, fy)?;
    let bottom_left = field.get(fx, fy + 1)?;
    let bottom_right = field.get(fx + 1, fy + 1)?;

    let flat = top_left == top_right && top_left == bottom_left && top_left == bottom_right;
    let dry = top_left > sea_level;
    Some(flat && dry)
}

#[cfg(test)]
mod tests {
    use super::{TerrainType, classify_vertex, is_buildable_face};
    use crate::terrain::HeightField;
    use alloc::vec;

    // Test-local thresholds: waterline at 0, a two-step shore band, mountains
    // from height 10. Concrete values so the classification boundaries are
    // legible; the shipped values are config (content.terrain.*, Phase 2).
    const SEA_LEVEL: i32 = 0;
    const SHORE_BAND: u32 = 2;
    const MOUNTAIN_MIN: i32 = 10;

    #[test]
    fn water_is_at_or_below_sea_level() {
        assert_eq!(
            classify_vertex(-3, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Water,
            "below the datum is underwater"
        );
        assert_eq!(
            classify_vertex(0, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Water,
            "a vertex exactly at sea level is water, not shore"
        );
    }

    #[test]
    fn shore_is_the_band_just_above_sea_level() {
        assert_eq!(
            classify_vertex(1, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Shore,
            "one step of dry land is shore"
        );
        assert_eq!(
            classify_vertex(2, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Shore,
            "the top of the band is still shore (inclusive)"
        );
    }

    #[test]
    fn land_is_above_the_shore_band_and_below_the_mountain_line() {
        assert_eq!(
            classify_vertex(3, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Land,
            "one step past the shore band is ordinary land"
        );
        assert_eq!(
            classify_vertex(9, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Land,
            "just below the mountain line is still land"
        );
    }

    #[test]
    fn mountain_is_at_or_above_the_threshold() {
        assert_eq!(
            classify_vertex(10, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Mountain,
            "exactly the threshold is mountain (inclusive)"
        );
        assert_eq!(
            classify_vertex(40, SEA_LEVEL, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Mountain
        );
    }

    #[test]
    fn mountain_precedence_beats_a_degenerately_wide_shore_band() {
        // A shore band wide enough to reach the mountain line must not steal a
        // high vertex: precedence is water → mountain → shore → land.
        assert_eq!(
            classify_vertex(10, SEA_LEVEL, 100, MOUNTAIN_MIN),
            TerrainType::Mountain,
            "height at the mountain line is mountain even under a huge shore band"
        );
    }

    #[test]
    fn a_zero_shore_band_leaves_no_shore() {
        assert_eq!(
            classify_vertex(1, SEA_LEVEL, 0, MOUNTAIN_MIN),
            TerrainType::Land,
            "with band 0, the first dry step is already land"
        );
        assert_eq!(
            classify_vertex(0, SEA_LEVEL, 0, MOUNTAIN_MIN),
            TerrainType::Water,
            "band 0 does not affect the waterline"
        );
    }

    #[test]
    fn a_nonzero_sea_level_shifts_the_whole_scale() {
        // Sea level is the datum, not a fixed 0: raise it and the same heights
        // reclassify around it.
        assert_eq!(
            classify_vertex(5, 5, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Water,
            "at the raised datum it is water"
        );
        assert_eq!(
            classify_vertex(6, 5, SHORE_BAND, MOUNTAIN_MIN),
            TerrainType::Shore,
            "one step above the raised datum is shore"
        );
    }

    #[test]
    fn a_flat_dry_face_is_buildable() {
        let field = HeightField::flat(3, 3, 5);
        assert_eq!(
            is_buildable_face(&field, 0, 0, SEA_LEVEL),
            Some(true),
            "four equal corners above sea level build"
        );
        assert_eq!(
            is_buildable_face(&field, 1, 1, SEA_LEVEL),
            Some(true),
            "every interior face of a flat dry field builds"
        );
    }

    #[test]
    fn a_flat_but_wet_face_is_not_buildable() {
        // Flat, but sitting exactly at the waterline: not dry, so not buildable
        // — the same boundary as the Water classification (at sea level = wet).
        let at_datum = HeightField::flat(2, 2, 0);
        assert_eq!(is_buildable_face(&at_datum, 0, 0, SEA_LEVEL), Some(false));
        let below = HeightField::flat(2, 2, -1);
        assert_eq!(is_buildable_face(&below, 0, 0, SEA_LEVEL), Some(false));
    }

    #[test]
    fn a_non_flat_face_is_not_buildable() {
        // One corner a step higher — dry, but not flat.
        let field = HeightField::from_cells(2, 2, vec![5, 5, 5, 6]).unwrap();
        assert_eq!(
            is_buildable_face(&field, 0, 0, SEA_LEVEL),
            Some(false),
            "a stepped face is not buildable"
        );
    }

    #[test]
    fn a_face_off_the_grid_is_none() {
        // Faces index by top-left corner; the last row/column of vertices have
        // no right/lower neighbour, so no face hangs off them.
        let field = HeightField::flat(2, 2, 5);
        assert_eq!(
            is_buildable_face(&field, 0, 0, SEA_LEVEL),
            Some(true),
            "the sole face of a 2×2 field is in range"
        );
        assert_eq!(
            is_buildable_face(&field, 1, 0, SEA_LEVEL),
            None,
            "no face spans off the right edge"
        );
        assert_eq!(
            is_buildable_face(&field, 0, 1, SEA_LEVEL),
            None,
            "no face spans off the bottom edge"
        );
    }
}
