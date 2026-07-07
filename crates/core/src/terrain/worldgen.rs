//! Seeded, parameterised world generation (ADR 0021, issue #7 Phase 2).
//!
//! [`generate`] is a **pure function of `sim.worldgen.seed`**: same seed + same
//! `sim.worldgen.*` ⇒ the same integer [`HeightField`], forever — no clock, no
//! I/O, no ambient randomness, and no floating-point (I3). It spans a
//! *shape × relief* space rather than baking one world in, so the same code
//! yields an island, a coastline, an archipelago, or a lake-dotted interior
//! (the Director's steer, ADR 0021 §2).
//!
//! The pipeline is **noise → shape mask → integer band → conform** (ADR 0021
//! §3):
//! 1. seeded multi-octave value noise gives each vertex an elevation *signal*;
//! 2. a per-[`Shape`] **mask** biases where land sits (a radial island falloff,
//!    an edge margin for a continent, a coarse blob field for an archipelago,
//!    or a full mask for a lake-dotted interior);
//! 3. the signal is thresholded to hit `land_percent` land and mapped into an
//!    integer height band around `sea_level`, spanning `relief` steps;
//! 4. a deterministic **conform pass**
//!    ([`HeightField::conform_to_step_invariant`]) lowers the field to the step
//!    invariant, so what `generate` returns already satisfies it.
//!
//! All arithmetic is integer fixed-point in `[0, ONE)`; the only writers of the
//! field are this module (via conform) and the shaping ops in [`super::shape`].

use alloc::vec::Vec;

use providence_config::{Shape, WorldgenParams};

use crate::rng::SplitMix64;

use super::field::{Height, HeightField};

/// Fixed-point fractional bits: signals and interpolation weights live in
/// `[0, ONE)` with this many bits of fraction. A power-of-two unit makes the
/// lattice-hash reduction a cheap bit-mask.
const FIXED_BITS: u32 = 16; // gate:allow(magic) fixed-point fractional bits

/// The fixed-point unit — `1.0` in the `[0, ONE)` representation.
const ONE: i64 = 1 << FIXED_BITS;

/// Percentage base: `land_percent` is a percentage, so a `/ PERCENT` turns it
/// into a fraction of the vertex count.
const PERCENT: i64 = 100; // gate:allow(magic) percent base

/// Amplitude/frequency falloff per octave: each octave doubles the frequency
/// and halves the amplitude — standard fractal value noise.
const OCTAVE_FALLOFF: u32 = 2; // gate:allow(magic) per-octave halving

/// Continent coastline margin, as a percentage of the shorter map side: the
/// band at the edge over which a continent recedes into the sea.
const CONTINENT_MARGIN_PERCENT: i64 = 20; // gate:allow(magic) continent edge margin (% of shorter side)

/// Archipelago mask wavelength, as a multiple of `feature_size`: the coarse
/// blob field that breaks the land into scattered clusters.
const ARCHIPELAGO_MASK_SCALE: u32 = 2; // gate:allow(magic) archipelago mask coarseness (× feature_size)

/// Odd 64-bit multipliers that scatter lattice coordinates into well-separated
/// hash seeds (distinct large odds, so `x`, `y`, and `octave` do not alias).
const HASH_X: u64 = 0x9E37_79B9_7F4A_7C15; // gate:allow(magic) lattice hash multiplier (x)
const HASH_Y: u64 = 0xC2B2_AE3D_27D4_EB4F; // gate:allow(magic) lattice hash multiplier (y)
const HASH_OCTAVE: u64 = 0x1656_67B1_9E37_79F9; // gate:allow(magic) lattice hash multiplier (octave)

/// Generate a world as an integer [`HeightField`] that already satisfies the
/// step invariant for `max_step` (ADR 0021).
///
/// Pure and deterministic: driven only by `params.seed` through the in-core
/// [`SplitMix64`] PRNG (ADR 0021 §1). `max_step` is the terrain step invariant
/// the field must satisfy (ADR 0017) — passed in rather than read from a
/// terrain param so worldgen stays a function of its own knobs plus the one
/// structural bound its output must honour.
#[must_use]
pub fn generate(params: &WorldgenParams, max_step: u32) -> HeightField {
    let (width, height) = (params.width, params.height);
    let count = width as usize * height as usize;

    // 1–2. Elevation signal per vertex: fractal noise shaped by the mode mask.
    let mut signals: Vec<i64> = Vec::with_capacity(count);
    for y in 0..height {
        for x in 0..width {
            let noise = fractal_noise(params, x, y);
            let mask = shape_mask(params, x, y);
            signals.push(noise * mask / ONE);
        }
    }

    // 3. Threshold to ~land_percent land, then map into the integer band.
    let cutoff = land_cutoff(&signals, params.land_percent);
    let max_signal = signals.iter().copied().max().unwrap_or(0);
    let cells: Vec<Height> = signals
        .iter()
        .map(|&signal| band_height(signal, cutoff, max_signal, params))
        .collect();

    // 4. Conform: lower the raw band into the step invariant.
    let mut field = HeightField::from_cells(width, height, cells)
        .expect("worldgen builds exactly width × height cells");
    field.conform_to_step_invariant(max_step);
    field
}

/// Summed multi-octave value noise at `(x, y)`, in `[0, ONE)`. Each octave
/// doubles the frequency and halves the amplitude; the sum is renormalised so a
/// flat all-`ONE` field would map back to `ONE`.
fn fractal_noise(params: &WorldgenParams, x: u32, y: u32) -> i64 {
    let mut sum: i64 = 0;
    let mut total: i64 = 0;
    let mut amplitude: i64 = ONE;
    let mut wavelength = params.feature_size.max(1);
    for octave in 0..params.detail {
        let sample = value_noise(params.seed, x, y, wavelength, octave);
        sum += sample * amplitude / ONE;
        total += amplitude;
        amplitude /= i64::from(OCTAVE_FALLOFF);
        wavelength = (wavelength / OCTAVE_FALLOFF).max(1);
        if amplitude == 0 {
            break; // finer octaves would add nothing
        }
    }
    if total == 0 {
        return 0; // detail == 0: no octaves contributed
    }
    sum * ONE / total
}

/// Bilinearly-interpolated value noise for one octave at `(x, y)`, in
/// `[0, ONE)`. Lattice corners are hashed from the seed and the octave index;
/// the fractional position within a cell interpolates between them.
fn value_noise(seed: u64, x: u32, y: u32, wavelength: u32, octave: u32) -> i64 {
    let w = i64::from(wavelength.max(1));
    let (gx, fx) = (i64::from(x) / w, i64::from(x) % w);
    let (gy, fy) = (i64::from(y) / w, i64::from(y) % w);

    let corner = |cx: i64, cy: i64| lattice(seed, cx, cy, octave);
    let top = lerp(corner(gx, gy), corner(gx + 1, gy), fx * ONE / w);
    let bottom = lerp(corner(gx, gy + 1), corner(gx + 1, gy + 1), fx * ONE / w);
    lerp(top, bottom, fy * ONE / w)
}

/// A stable pseudo-random value in `[0, ONE)` at integer lattice point
/// `(gx, gy)` for `octave`, derived purely from `seed` (ADR 0021 §1).
fn lattice(seed: u64, gx: i64, gy: i64, octave: u32) -> i64 {
    // Lattice coordinates are non-negative (cell index of a non-negative
    // vertex), so the unsigned view is exact.
    let ux = u64::try_from(gx).unwrap_or_default();
    let uy = u64::try_from(gy).unwrap_or_default();
    let mixed = seed
        ^ ux.wrapping_mul(HASH_X)
        ^ uy.wrapping_mul(HASH_Y)
        ^ u64::from(octave).wrapping_mul(HASH_OCTAVE);
    let draw = SplitMix64::new(mixed).next_u64();
    let mask = (1_u64 << FIXED_BITS) - 1;
    i64::from(u32::try_from(draw & mask).unwrap_or_default())
}

/// Linear interpolation between `a` and `b` by `t` in `[0, ONE)`, in
/// fixed-point. `a`, `b`, `t` all sit in `[0, ONE)`, so the product fits `i64`.
fn lerp(a: i64, b: i64, t: i64) -> i64 {
    a + (b - a) * t / ONE
}

/// The per-[`Shape`] mask at `(x, y)`, in `[0, ONE]`: how strongly this vertex
/// is biased toward land before thresholding (ADR 0021 §3).
fn shape_mask(params: &WorldgenParams, x: u32, y: u32) -> i64 {
    match params.shape {
        Shape::Island => island_mask(params, x, y),
        Shape::Continent => continent_mask(params, x, y),
        Shape::Archipelago => value_noise(
            params.seed,
            x,
            y,
            params.feature_size.saturating_mul(ARCHIPELAGO_MASK_SCALE),
            params.detail, // a fresh octave index, independent of the base noise
        ),
        Shape::Inland => ONE, // full mask: only the lowest ground becomes lakes
    }
}

/// Radial falloff for [`Shape::Island`]: `ONE` at the centre, `0` past the
/// inscribed radius, using squared normalised distance so the land reads as a
/// rounded mass ringed by sea.
fn island_mask(params: &WorldgenParams, x: u32, y: u32) -> i64 {
    // Centre = the grid midpoint; distance is normalised so the inscribed
    // radius maps to ONE.
    let half_w = i64::from(params.width / 2).max(1); // gate:allow(magic) midpoint divisor
    let half_h = i64::from(params.height / 2).max(1); // gate:allow(magic) midpoint divisor
    let dx = (i64::from(x) - half_w).abs() * ONE / half_w;
    let dy = (i64::from(y) - half_h).abs() * ONE / half_h;
    let distance_sq = (dx * dx + dy * dy) / ONE;
    ONE - distance_sq.min(ONE)
}

/// Edge-margin falloff for [`Shape::Continent`]: full land in the interior,
/// receding to sea over a margin at every edge so a coastline is guaranteed.
fn continent_mask(params: &WorldgenParams, x: u32, y: u32) -> i64 {
    let shorter = i64::from(params.width.min(params.height));
    let margin = (shorter * CONTINENT_MARGIN_PERCENT / PERCENT).max(1);
    let to_edge = |v: u32, size: u32| i64::from(v.min(size.saturating_sub(1).saturating_sub(v)));
    let edge_distance = to_edge(x, params.width).min(to_edge(y, params.height));
    (edge_distance * ONE / margin).min(ONE)
}

/// The signal cutoff that lands about `land_percent` of vertices dry: the
/// value at the `(100 − land_percent)` percentile of the sorted signals
/// (ADR 0021 §3). Vertices at or above it are land.
fn land_cutoff(signals: &[i64], land_percent: u32) -> i64 {
    let mut sorted: Vec<i64> = signals.to_vec();
    sorted.sort_unstable();
    let n = i64::try_from(sorted.len()).unwrap_or(i64::MAX);
    // Index from the bottom of the (100 − land_percent) fraction; clamped inside
    // the slice so land_percent at either extreme still indexes a real value.
    let below = (PERCENT - i64::from(land_percent)).clamp(0, PERCENT);
    let index = usize::try_from(below * n / PERCENT)
        .unwrap_or_default()
        .min(sorted.len().saturating_sub(1));
    sorted[index]
}

/// Map one elevation `signal` into an integer height around `sea_level`
/// (ADR 0021 §3): land at or above `cutoff` rises up to `relief` steps above the
/// waterline; everything below `cutoff` is a **flat sea floor at `sea_level`**.
///
/// The flat floor is deliberate. The conform pass only lowers, so a sloping
/// seabed would terrace *up* toward the peak and drag the coast under, pulling
/// the dry share well below `land_percent`. Pinning water at the datum means
/// conform can never lower land below `sea_level + 1`, so the dry share tracks
/// `land_percent` and the land keeps its relief — the sea is simply the sea.
fn band_height(signal: i64, cutoff: i64, max_signal: i64, params: &WorldgenParams) -> Height {
    let sea_level = i64::from(params.sea_level);
    let relief = i64::from(params.relief.max(1));
    let height = if signal >= cutoff {
        // Shore (sea_level + 1) up to sea_level + relief.
        let span = (max_signal - cutoff).max(1);
        let rise = (signal - cutoff) * (relief - 1) / span;
        sea_level + 1 + rise
    } else {
        sea_level // flat sea floor at the waterline datum
    };
    as_height(height)
}

/// Saturating `i64 → Height` conversion (mirrors the shaping ops): worldgen
/// heights sit far from the range ends, so this only bites a pathological
/// config.
fn as_height(value: i64) -> Height {
    Height::try_from(value).unwrap_or(if value < 0 { Height::MIN } else { Height::MAX })
}

#[cfg(test)]
mod tests {
    use super::generate;
    use crate::terrain::{TerrainType, classify_vertex};
    use providence_config::{Shape, WorldgenParams};

    const MAX_STEP: u32 = 1;
    const SHORE_BAND: u32 = 2;
    const MOUNTAIN_MIN: i32 = 12;

    /// A reasonable default-ish world for the tests: a mid-size island with
    /// mixed relief, mirroring the shipped `config/default.toml` shape.
    fn island_params(seed: u64) -> WorldgenParams {
        WorldgenParams {
            width: 48,
            height: 48,
            seed,
            sea_level: 0,
            land_percent: 55,
            shape: Shape::Island,
            relief: 12,
            feature_size: 16,
            detail: 3,
        }
    }

    /// Count vertices of each terrain type over the whole field.
    fn census(params: &WorldgenParams) -> (u32, u32) {
        let field = generate(params, MAX_STEP);
        let mut water = 0;
        let mut land = 0;
        for y in 0..field.height() {
            for x in 0..field.width() {
                let h = field.get(x, y).unwrap();
                match classify_vertex(h, params.sea_level, SHORE_BAND, MOUNTAIN_MIN) {
                    TerrainType::Water => water += 1,
                    _ => land += 1,
                }
            }
        }
        (water, land)
    }

    #[test]
    fn generated_field_has_the_requested_dimensions() {
        let field = generate(&island_params(1), MAX_STEP);
        assert_eq!((field.width(), field.height()), (48, 48));
    }

    #[test]
    fn generated_field_satisfies_the_step_invariant() {
        // The conform pass is the whole point: whatever the noise, the field
        // handed back is always invariant-valid (ADR 0021 §3).
        for seed in 0..8 {
            let field = generate(&island_params(seed), MAX_STEP);
            assert!(
                field.satisfies_step_invariant(MAX_STEP),
                "seed {seed} produced a field that violates the step invariant"
            );
        }
    }

    #[test]
    fn generation_is_deterministic_for_a_seed() {
        let a = generate(&island_params(42), MAX_STEP);
        let b = generate(&island_params(42), MAX_STEP);
        assert_eq!(a, b, "same seed + params must produce the same field (I3)");
    }

    #[test]
    fn different_seeds_produce_different_worlds() {
        let a = generate(&island_params(1), MAX_STEP);
        let b = generate(&island_params(2), MAX_STEP);
        assert_ne!(a, b, "the seed must vary the instance");
    }

    #[test]
    fn an_island_is_ringed_by_sea() {
        // Every edge vertex of an island sits at or below sea level: the radial
        // mask pulls the coast inward on all sides.
        let params = island_params(7);
        let field = generate(&params, MAX_STEP);
        let (w, h) = (field.width(), field.height());
        for x in 0..w {
            assert!(
                field.get(x, 0).unwrap() <= params.sea_level,
                "top edge is sea"
            );
            assert!(
                field.get(x, h - 1).unwrap() <= params.sea_level,
                "bottom edge is sea"
            );
        }
        for y in 0..h {
            assert!(
                field.get(0, y).unwrap() <= params.sea_level,
                "left edge is sea"
            );
            assert!(
                field.get(w - 1, y).unwrap() <= params.sea_level,
                "right edge is sea"
            );
        }
    }

    #[test]
    fn an_island_has_land_at_its_centre() {
        let params = island_params(7);
        let field = generate(&params, MAX_STEP);
        let centre = field.get(params.width / 2, params.height / 2).unwrap();
        assert!(centre > params.sea_level, "the island's centre is dry land");
    }

    #[test]
    fn land_percent_moves_the_water_share() {
        // A drier target must not yield more water than a wetter one — the
        // threshold tracks land_percent (approximately, before conform).
        let mut wet = island_params(3);
        wet.land_percent = 30;
        let mut dry = island_params(3);
        dry.land_percent = 80;
        let (wet_water, _) = census(&wet);
        let (dry_water, _) = census(&dry);
        assert!(
            dry_water < wet_water,
            "80% land must leave less water than 30% land ({dry_water} vs {wet_water})"
        );
    }

    #[test]
    fn relief_raises_the_high_ground() {
        // More relief must produce a taller peak somewhere on the map.
        let peak = |relief: i32| {
            let mut params = island_params(9);
            params.relief = relief;
            let field = generate(&params, MAX_STEP);
            let mut max = i32::MIN;
            for y in 0..field.height() {
                for x in 0..field.width() {
                    max = max.max(field.get(x, y).unwrap());
                }
            }
            max
        };
        assert!(
            peak(20) > peak(4),
            "greater relief must lift the highest ground"
        );
    }

    #[test]
    fn every_shape_generates_a_valid_world() {
        // Each mode produces some land and some water, and always conforms.
        for shape in [
            Shape::Island,
            Shape::Continent,
            Shape::Archipelago,
            Shape::Inland,
        ] {
            let mut params = island_params(5);
            params.shape = shape;
            let field = generate(&params, MAX_STEP);
            assert!(
                field.satisfies_step_invariant(MAX_STEP),
                "{shape:?} violated the invariant"
            );
            let (water, land) = census(&params);
            assert!(water > 0, "{shape:?} produced no water");
            assert!(land > 0, "{shape:?} produced no land");
        }
    }

    #[test]
    fn inland_is_wetter_than_it_is_dry_only_at_low_land_percent() {
        // Inland keeps a full mask, so at a high land_percent it is mostly land
        // with only interior lakes — more land than water.
        let mut params = island_params(11);
        params.shape = Shape::Inland;
        params.land_percent = 85;
        let (water, land) = census(&params);
        assert!(land > water, "a high-land inland world is mostly dry");
    }

    #[test]
    fn a_bigger_map_conforms_too() {
        // Exercise a non-square, larger grid to catch index/scaling errors the
        // square fixtures would hide.
        let params = WorldgenParams {
            width: 80,
            height: 40,
            ..island_params(13)
        };
        let field = generate(&params, MAX_STEP);
        assert_eq!((field.width(), field.height()), (80, 40));
        assert!(field.satisfies_step_invariant(MAX_STEP));
    }
}
