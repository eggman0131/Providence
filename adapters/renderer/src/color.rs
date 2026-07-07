//! Terrain type → colour: the material table (ADR 0023; issue #22 Phase 1).
//!
//! Pure and GPU-free. The flat-shaded renderer colours each vertex by its
//! **derived terrain type** (ADR 0017 §1), read from the snapshot the app hands
//! across — not by a free height ramp. So what the land *is* drives what it
//! looks like: sand at the shore, grass on the lowland, bare rock on the heights
//! ramping to snow at the peaks. Because the bands trace the terrain-type
//! boundaries, they trace the integer step boundaries too — sharpening (not
//! hiding) the stepped model still under judgment (#11). The colours are config
//! (`render.material.*`), so a designer retunes the look without touching code
//! (I1). Kept here and unit-tested so the table is correct before any GPU code.

use providence_config::MaterialParams;
use providence_ports::TerrainType;

/// Linear-RGB colour for a vertex of terrain type `kind` at integer `height`.
///
/// Each terrain type takes its flat base colour from `material`; **Mountain**
/// additionally ramps from `mountain_rgb` at the mountain band's lowest vertex
/// (`mountain_lo`) to `peak_rgb` at its highest (`mountain_hi`), so snow gathers
/// toward the peaks. `mountain_lo`/`mountain_hi` are the height bounds of the
/// mountain vertices actually present in the frame (the drawn range), computed
/// by the mesh builder; a degenerate range (`mountain_lo >= mountain_hi`)
/// collapses the ramp to `mountain_rgb`.
#[must_use]
pub fn material_color(
    kind: TerrainType,
    height: i32,
    mountain_lo: i32,
    mountain_hi: i32,
    material: &MaterialParams,
) -> [f32; 3] {
    match kind {
        TerrainType::Water => material.water_rgb,
        TerrainType::Shore => material.shore_rgb,
        TerrainType::Land => material.land_rgb,
        TerrainType::Mountain => {
            let t = normalized_height(height, mountain_lo, mountain_hi);
            lerp_rgb(material.mountain_rgb, material.peak_rgb, t)
        }
    }
}

/// Position of `height` within `[min, max]` as a fraction in `[0, 1]`.
///
/// Heights are clamped into the range, so out-of-range values saturate at the
/// nearest anchor rather than extrapolating. A degenerate range
/// (`min >= max`) collapses to `0.0`.
fn normalized_height(height: i32, min: i32, max: i32) -> f32 {
    if max <= min {
        return 0.0;
    }
    let span = (max - min) as f32;
    let offset = (height.clamp(min, max) - min) as f32;
    offset / span
}

/// Component-wise linear interpolation between two RGB colours.
fn lerp_rgb(low: [f32; 3], high: [f32; 3], t: f32) -> [f32; 3] {
    [
        low[0] + (high[0] - low[0]) * t,
        low[1] + (high[1] - low[1]) * t,
        low[2] + (high[2] - low[2]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::{material_color, normalized_height};
    use providence_config::MaterialParams;
    use providence_ports::TerrainType;

    /// A material table with a distinct colour per type so a returned colour
    /// names the band unambiguously; the mountain ramp runs black→white.
    const MATERIAL: MaterialParams = MaterialParams {
        water_rgb: [0.0, 0.0, 1.0],
        shore_rgb: [1.0, 1.0, 0.0],
        land_rgb: [0.0, 1.0, 0.0],
        mountain_rgb: [0.0, 0.0, 0.0],
        peak_rgb: [1.0, 1.0, 1.0],
    };

    /// Floats compared within a tolerance (clippy forbids `==` on floats).
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    /// Element-wise [`approx`] for an RGB triple.
    fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-5)
    }

    #[test]
    fn each_flat_type_takes_its_base_colour() {
        // Water/shore/land are flat bands: their colour ignores height and the
        // mountain range entirely.
        assert!(approx3(
            material_color(TerrainType::Water, -5, 10, 20, &MATERIAL),
            [0.0, 0.0, 1.0]
        ));
        assert!(approx3(
            material_color(TerrainType::Shore, 1, 10, 20, &MATERIAL),
            [1.0, 1.0, 0.0]
        ));
        assert!(approx3(
            material_color(TerrainType::Land, 4, 10, 20, &MATERIAL),
            [0.0, 1.0, 0.0]
        ));
    }

    #[test]
    fn mountains_ramp_from_rock_at_the_base_to_snow_at_the_peak() {
        // Across the mountain band [10, 20]: the base is bare rock, the top is
        // snow, the middle is halfway.
        let color = |h| material_color(TerrainType::Mountain, h, 10, 20, &MATERIAL);
        assert!(
            approx3(color(10), [0.0, 0.0, 0.0]),
            "lowest mountain = rock"
        );
        assert!(
            approx3(color(20), [1.0, 1.0, 1.0]),
            "highest mountain = snow"
        );
        assert!(
            approx3(color(15), [0.5, 0.5, 0.5]),
            "middle = halfway to snow"
        );
    }

    #[test]
    fn a_degenerate_mountain_range_is_all_rock() {
        // Every mountain vertex at the same height: no ramp, so it is all rock
        // (the ramp fraction collapses to 0).
        assert!(approx(normalized_height(12, 12, 12), 0.0));
        assert!(approx3(
            material_color(TerrainType::Mountain, 12, 12, 12, &MATERIAL),
            [0.0, 0.0, 0.0]
        ));
    }

    #[test]
    fn out_of_range_mountain_heights_saturate_at_the_anchors() {
        // A height below/above the band saturates, never extrapolates past the
        // rock/snow anchors.
        assert!(approx3(
            material_color(TerrainType::Mountain, 4, 10, 20, &MATERIAL),
            [0.0, 0.0, 0.0]
        ));
        assert!(approx3(
            material_color(TerrainType::Mountain, 99, 10, 20, &MATERIAL),
            [1.0, 1.0, 1.0]
        ));
    }
}
