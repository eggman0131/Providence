//! Terrain surface geometry (issue #8 Phase 1 mesh).
//!
//! Pure and GPU-free. The flat-shaded stepped mesh — per-face vertices with
//! per-face normals, so each integer step reads as a crisp facet (issue #8
//! decision) — is assembled in Phase 1 from these building blocks. Pre-work
//! provides and tests the coordinate kernel: mapping a [`TerrainFrame`] vertex
//! `(x, y, height)` to a centred world-space position.
//!
//! `vertical_scale` (how tall one integer height step is, in world units) is a
//! caller-supplied parameter here, not a constant: the Phase-1 renderer will
//! source it from a `render.*` key once it draws the mesh for real.

use providence_ports::TerrainFrame;

/// A world-space position, `[x, y, z]`, with y up.
pub type Position = [f32; 3];

/// World-space position of grid vertex `(x, y)` at `height`.
///
/// The grid lies on the world x/z plane and is **centred** on the origin so the
/// camera orbits the middle of the map; height becomes the up (y) axis, scaled
/// by `vertical_scale`.
#[must_use]
pub fn vertex_position(
    x: u32,
    y: u32,
    height: i32,
    width: u32,
    depth: u32,
    vertical_scale: f32,
) -> Position {
    [
        center_offset(x, width),
        height as f32 * vertical_scale,
        center_offset(y, depth),
    ]
}

/// Offset of index `i` from the centre of a `size`-wide axis, in units of one
/// vertex spacing, so a `size`-wide grid spans `[-(size-1)/2, (size-1)/2]`.
fn center_offset(i: u32, size: u32) -> f32 {
    i as f32 - (size.saturating_sub(1) as f32) / 2.0
}

/// The centred world positions of every vertex in `frame`, row-major — the
/// vertex grid the Phase-1 mesh builder facets. A vertex missing from the
/// snapshot is skipped (a well-formed frame skips none).
#[must_use]
pub fn vertex_positions(frame: &TerrainFrame<'_>, vertical_scale: f32) -> Vec<Position> {
    let (width, depth) = (frame.width(), frame.height());
    let mut positions = Vec::with_capacity(width as usize * depth as usize);
    for y in 0..depth {
        for x in 0..width {
            if let Some(height) = frame.get(x, y) {
                positions.push(vertex_position(x, y, height, width, depth, vertical_scale));
            }
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::{center_offset, vertex_position, vertex_positions};
    use providence_ports::TerrainFrame;

    /// Floats compared within a tolerance (clippy forbids `==` on floats).
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    /// Element-wise [`approx`] for a world-space position.
    fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-5)
    }

    #[test]
    fn an_odd_axis_is_centred_on_the_origin() {
        assert!(approx(center_offset(1, 3), 0.0), "middle of a 3-wide axis");
        assert!(approx(center_offset(0, 3), -1.0));
        assert!(approx(center_offset(2, 3), 1.0));
    }

    #[test]
    fn height_becomes_the_scaled_up_axis() {
        let pos = vertex_position(1, 1, 4, 3, 3, 0.5);
        assert!(
            approx3(pos, [0.0, 2.0, 0.0]),
            "centre vertex, height 4 × scale 0.5"
        );
    }

    #[test]
    fn positions_cover_every_vertex_row_major() {
        let heights = [0, 1, 1, 2]; // 2×2
        let frame = TerrainFrame::new(2, 2, &heights);
        let positions = vertex_positions(&frame, 1.0);
        assert_eq!(positions.len(), 4);
        assert!(approx(positions[0][1], 0.0), "(0,0) height 0 → y 0");
        assert!(approx(positions[3][1], 2.0), "(1,1) height 2 → y 2");
    }
}
