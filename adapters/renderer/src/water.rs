//! The water-surface plane geometry (ADR 0023, Phase 2; issue #22).
//!
//! Pure and GPU-free. Builds the flat sheet the renderer floats at the waterline
//! as a **living, depth-cued water surface**: a translucent plane spanning the
//! whole grid, alpha-blended over the terrain so land rising above the waterline
//! reveals the coastline (the shoreline tracks a shaping edit *for free*, derived
//! against the live terrain every frame). The colour, translucency, lift, and
//! shimmer are all `render.water.*` config and live in the shader/uniforms
//! ([`crate::gpu`]); the **plane geometry** — and the per-vertex *water-column
//! depth* the shader cues the deep-water colour off — lives here, so it is
//! unit-tested in the gate before any GPU code.
//!
//! The surface itself is always one flat height (the waterline datum): every
//! vertex shares [`surface_y`](WaterPlane::surface_y). What varies is the
//! **depth of water beneath it** — how far the seabed sits below the surface —
//! which the plane carries per vertex so the shader can darken and thicken the
//! water over the deeps. That is the Director's ruling made visible: you cannot
//! dig a hole in the *surface* (it never moves); a dug pit shows instead as
//! *deeper water* (a darker, more opaque patch), never a see-through crater.
//!
//! The plane is centred on the origin exactly like the terrain mesh
//! ([`crate::mesh`]), so the two line up. Worldgen pins the sea floor **flat at
//! the waterline datum**, so the sheet would be coplanar with the seabed; a small
//! `surface_lift` floats it just above (no z-fighting, and a hair of body),
//! deliberately kept below one height step so it never rises over the first dry
//! shore.

use providence_ports::Height;

use crate::mesh::{Position, vertex_position};

/// A water-plane vertex: its world position (all vertices share the flat surface
/// height) and the **water-column depth** at that grid point — how far, in
/// integer height steps, the seabed sits below the surface. `0` at the shoreline
/// and over dry land; larger over a deep or dug-out seabed. The shader ramps the
/// surface from the shallow colour/opacity to the deep one over the first
/// `render.water.depth_full` steps of this depth (ADR 0023, Phase 2 refinement).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterVertex {
    /// Centred world position; `position[1]` is the shared flat surface height.
    pub position: Position,
    /// Water-column depth in height steps (`max(0, waterline - seabed_height)`).
    pub depth: f32,
}

/// The water-surface geometry: a triangle-list tessellation of the grid at the
/// (lifted) waterline height, each vertex carrying its water-column depth — the
/// mesh the renderer uploads for the alpha-blended, depth-cued water pass
/// (ADR 0023, Phase 2).
#[derive(Clone, Debug, PartialEq)]
pub struct WaterPlane {
    /// Triangle-list vertices, two triangles per grid cell (`p00,p10,p11` then
    /// `p00,p11,p01`), none shared — flat in Y, varying in `depth`.
    vertices: Vec<WaterVertex>,
    /// The world-space Y every vertex sits at — the lifted waterline height.
    /// Kept explicitly so it is well-defined even for a degenerate (empty) grid.
    surface_y: f32,
}

impl WaterPlane {
    /// Build the water plane for a `width × depth` grid whose sea-level datum is
    /// `waterline`, over the row-major seabed `heights`, matching the terrain
    /// mesh's `vertical_scale` centring.
    ///
    /// The surface sits at `waterline * vertical_scale + surface_lift` in world
    /// Y — the small positive `surface_lift` floats it clear of the coplanar flat
    /// seabed (ADR 0023, Phase 2). It tessellates exactly the terrain's grid
    /// (`vertex_position` at each vertex), so the sea meets the land edge to edge,
    /// and every vertex carries its water-column depth (`max(0, waterline -
    /// height)`) so the shader can cue deep water darker. A grid narrower or
    /// shallower than two vertices — or a `heights` slice too short to cover it —
    /// yields an empty plane (nothing to draw), never a panic.
    #[must_use]
    pub fn new(
        width: u32,
        depth: u32,
        heights: &[Height],
        waterline: i32,
        vertical_scale: f32,
        surface_lift: f32,
    ) -> Self {
        let surface_y = waterline as f32 * vertical_scale + surface_lift;
        let mut vertices = Vec::new();

        // The seabed depth (in height steps) below the surface at grid (x, y);
        // 0 at or above the waterline (dry land, drawn but occluded by terrain).
        let column = |x: u32, y: u32| -> Option<f32> {
            let height = *heights.get((y * width + x) as usize)?;
            Some((waterline - height).max(0) as f32)
        };
        let corner = |x: u32, y: u32| -> Option<WaterVertex> {
            let mut position = vertex_position(x, y, 0, width, depth, vertical_scale);
            position[1] = surface_y; // the surface is flat; only depth varies
            Some(WaterVertex {
                position,
                depth: column(x, y)?,
            })
        };

        for y in 0..depth.saturating_sub(1) {
            for x in 0..width.saturating_sub(1) {
                let (Some(c00), Some(c10), Some(c01), Some(c11)) = (
                    corner(x, y),
                    corner(x + 1, y),
                    corner(x, y + 1),
                    corner(x + 1, y + 1),
                ) else {
                    continue; // a heights slice too short skips the cell, never panics
                };
                vertices.extend_from_slice(&[c00, c10, c11, c00, c11, c01]);
            }
        }

        Self {
            vertices,
            surface_y,
        }
    }

    /// The triangle-list vertices to upload (position + water-column depth).
    #[must_use]
    pub fn vertices(&self) -> &[WaterVertex] {
        &self.vertices
    }

    /// The world-space Y the whole surface sits at — the lifted waterline height.
    /// Handy for tests and callers that need the datum.
    #[must_use]
    pub fn surface_y(&self) -> f32 {
        self.surface_y
    }

    /// Whether the plane has no geometry (a grid smaller than one cell, or a
    /// `heights` slice too short to cover it).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::WaterPlane;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    #[test]
    fn the_surface_is_one_flat_height_across_every_vertex() {
        // Waterline 3 at scale 2 → seabed Y 6; a 0.2 lift floats the sheet to 6.2.
        // The seabed varies underneath, but the surface height must not.
        let heights = [3, 1, -4, 3, 3, 0, 3, 3, 3]; // 3×3, mixed depths
        let plane = WaterPlane::new(3, 3, &heights, 3, 2.0, 0.2);
        assert!(approx(plane.surface_y(), 6.2), "waterline*scale + lift");
        for vertex in plane.vertices() {
            assert!(
                approx(vertex.position[1], 6.2),
                "the water surface is flat everywhere"
            );
        }
    }

    #[test]
    fn a_vertex_carries_the_seabed_depth_below_the_waterline() {
        // Waterline 0; one vertex dug to -5 → its column depth is 5 steps, while
        // a vertex at the waterline (and dry land above it) reads 0.
        let heights = [0, -5, 2, 0]; // 2×2: (0,0)=0, (1,0)=-5, (0,1)=+2 land, (1,1)=0
        let plane = WaterPlane::new(2, 2, &heights, 0, 1.0, 0.0);
        let depths: Vec<f32> = plane.vertices().iter().map(|v| v.depth).collect();
        assert!(
            depths.iter().any(|&d| approx(d, 5.0)),
            "the dug vertex is 5 deep"
        );
        assert!(
            depths.iter().all(|&d| d >= 0.0),
            "depth never goes negative — dry land clamps to 0, not a lift"
        );
        assert!(
            depths.iter().any(|&d| approx(d, 0.0)),
            "the land/waterline vertices read 0 depth"
        );
    }

    #[test]
    fn depth_scales_with_the_waterline_not_the_vertical_scale() {
        // Depth is measured in height STEPS, so a taller vertical_scale (which
        // only exaggerates the LOOK) must not change the reported column depth.
        let heights = [0, -4, 0, 0];
        let flat = WaterPlane::new(2, 2, &heights, 0, 1.0, 0.0);
        let tall = WaterPlane::new(2, 2, &heights, 0, 5.0, 0.0);
        let deepest = |p: &WaterPlane| p.vertices().iter().map(|v| v.depth).fold(0.0_f32, f32::max);
        assert!(
            approx(deepest(&flat), 4.0) && approx(deepest(&tall), 4.0),
            "4 steps deep regardless of the vertical exaggeration"
        );
    }

    #[test]
    fn the_plane_spans_the_centred_grid_extent() {
        // A 5-wide, 3-deep grid is centred on the origin, so it spans x in
        // [-2, 2] and z in [-1, 1] — exactly the terrain mesh's extent.
        let heights = [0; 15];
        let plane = WaterPlane::new(5, 3, &heights, 0, 1.0, 0.0);
        let xs: Vec<f32> = plane.vertices().iter().map(|v| v.position[0]).collect();
        let zs: Vec<f32> = plane.vertices().iter().map(|v| v.position[2]).collect();
        let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min_z = zs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_z = zs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(approx(min_x, -2.0) && approx(max_x, 2.0), "x spans [-2, 2]");
        assert!(approx(min_z, -1.0) && approx(max_z, 1.0), "z spans [-1, 1]");
    }

    #[test]
    fn it_tessellates_two_triangles_per_cell() {
        // 4×4 vertices → 3×3 cells → 9 cells × 2 triangles × 3 vertices = 54.
        let heights = [0; 16];
        let plane = WaterPlane::new(4, 4, &heights, 0, 1.0, 0.1);
        assert_eq!(plane.vertices().len(), 54, "two triangles per grid cell");
        assert!(!plane.is_empty());
    }

    #[test]
    fn a_zero_lift_places_the_surface_on_the_seabed_datum() {
        // With no lift the surface is exactly at waterline*scale — coplanar with
        // the flat seabed (the config default lifts it to avoid z-fighting).
        let heights = [-2; 16];
        let plane = WaterPlane::new(4, 4, &heights, -2, 3.0, 0.0);
        assert!(approx(plane.surface_y(), -6.0), "waterline -2 × scale 3");
    }

    #[test]
    fn a_degenerate_grid_collapses_without_panicking() {
        // A single-column grid has no cell → an empty plane, but the call is
        // total (no u32 underflow) and the surface height is still defined.
        let plane = WaterPlane::new(1, 1, &[0], 0, 1.0, 0.1);
        assert!(plane.is_empty(), "no cell → nothing to draw");
        assert!(approx(plane.surface_y(), 0.1), "the datum is still defined");
    }

    #[test]
    fn a_short_heights_slice_skips_cells_without_panicking() {
        // A heights slice too short to cover the grid must not index out of
        // bounds — the uncovered cells are simply skipped.
        let plane = WaterPlane::new(4, 4, &[0, 1, 2], 0, 1.0, 0.0);
        assert!(
            plane.is_empty(),
            "no fully-covered cell → empty, not a panic"
        );
    }
}
