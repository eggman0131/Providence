//! Screen-ray vertex picking (ADR 0020 §3; issue #8 Phase 3).
//!
//! Pure and GPU-free. Turns a point on the screen into a world-space ray from
//! the camera, then finds the grid vertex that ray passes closest to — the
//! "which vertex is under the crosshair" question the Phase-3 readout answers,
//! and the exact resolve #9 reuses to turn a click into a raise/lower command.
//!
//! It is **read-only** view maths, entirely adapter-local (ADR 0020 §3): the
//! camera's floats resolve to a ray here, at the edge; nothing it computes ever
//! crosses back into the deterministic core. Kept here and unit-tested so the
//! ray/pick geometry is correct without any GPU.

use providence_ports::TerrainFrame;

use crate::camera::Camera;
use crate::math::{Vec3, cross, dot, normalize, sub};
use crate::mesh::vertex_position;

/// An owned copy of a presented [`TerrainFrame`]'s grid — dimensions plus the
/// row-major heights — kept by a renderer so it can pick a vertex every frame
/// as the camera moves (the borrowed [`TerrainFrame`] handed to `present` does
/// not outlive the call). Read-only presentation state (ADR 0020 §1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridSnapshot {
    /// Grid columns.
    pub width: u32,
    /// Grid rows.
    pub height: u32,
    /// Row-major heights, `width * height` of them.
    pub heights: Vec<i32>,
}

impl GridSnapshot {
    /// Take an owned snapshot of a presented frame.
    #[must_use]
    pub fn from_frame(frame: &TerrainFrame<'_>) -> Self {
        Self {
            width: frame.width(),
            height: frame.height(),
            heights: frame.heights().to_vec(),
        }
    }

    /// Borrow this snapshot back as a [`TerrainFrame`] for picking. Picking reads
    /// only heights, so this is a **heights-only** frame — its `types` slice is
    /// empty (ADR 0023) and its waterline is an unread `0` (ADR 0023, Phase 2);
    /// nothing here ever colours a vertex or draws water.
    #[must_use]
    pub fn frame(&self) -> TerrainFrame<'_> {
        TerrainFrame::new(self.width, self.height, &self.heights, &[], 0)
    }
}

/// A world-space ray: an `origin` and a **unit** `direction`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Where the ray starts (the camera eye).
    pub origin: Vec3,
    /// Unit direction the ray travels.
    pub direction: Vec3,
}

/// The grid vertex a ray resolved to: its integer grid coordinate and the
/// height it carries. The `(x, y)` #9 will raise/lower; the `height` the
/// readout shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickedVertex {
    /// Grid column.
    pub x: u32,
    /// Grid row.
    pub y: u32,
    /// Integer height at `(x, y)`.
    pub height: i32,
}

/// The ray through the **screen centre** — the reticle the Phase-3 readout
/// identifies a vertex under. Just [`screen_ray`] at normalised device
/// coordinate `(0, 0)`; named because the reticle is the Phase-3 case.
#[must_use]
pub fn reticle_ray(camera: &Camera, aspect: f32) -> Ray {
    screen_ray(camera, aspect, [0.0, 0.0])
}

/// Convert a cursor position in **physical pixels** (origin top-left, y down —
/// `winit`'s convention) into the normalised device coordinate [`screen_ray`]
/// expects: `[0, 0]` is the screen centre, `[-1, -1]` the bottom-left, `[1, 1]`
/// the top-right (y up). This is the cursor-tracked generalisation of the
/// Phase-3 reticle: issue #9 picks the vertex under the *live cursor*, not just
/// the centre. A degenerate zero-sized surface maps to the centre.
#[must_use]
pub fn cursor_ndc(cursor_px: (f32, f32), size: (u32, u32)) -> [f32; 2] {
    let (px, py) = cursor_px;
    let (width, height) = size;
    if width == 0 || height == 0 {
        return [0.0, 0.0];
    }
    [
        2.0 * px / width as f32 - 1.0,
        // Flip y: pixels grow downward, NDC grows upward.
        1.0 - 2.0 * py / height as f32,
    ]
}

/// The world-space ray from the camera through a point given in normalised
/// device coordinates: `ndc = [0, 0]` is the screen centre, `[-1, -1]` the
/// bottom-left, `[1, 1]` the top-right (y up). This is the general form #9
/// drives from the cursor; Phase 3 only needs the centre ([`reticle_ray`]).
///
/// The pinhole construction avoids a matrix inverse: it offsets the view
/// direction across the camera's right/up axes by the half-extents of the
/// frustum at the given `ndc`, matching the `perspective_rh` lens
/// ([`crate::math`]).
#[must_use]
pub fn screen_ray(camera: &Camera, aspect: f32, ndc: [f32; 2]) -> Ray {
    let forward = normalize(sub(camera.target, camera.eye));
    let right = normalize(cross(forward, camera.up));
    let true_up = cross(right, forward);
    let half_height = (camera.fov_y_radians / 2.0).tan();
    let half_width = half_height * aspect;
    let direction = normalize([
        forward[0] + ndc[0] * half_width * right[0] + ndc[1] * half_height * true_up[0],
        forward[1] + ndc[0] * half_width * right[1] + ndc[1] * half_height * true_up[1],
        forward[2] + ndc[0] * half_width * right[2] + ndc[1] * half_height * true_up[2],
    ]);
    Ray {
        origin: camera.eye,
        direction,
    }
}

/// The grid vertex the `ray` strikes: cast it against the drawn terrain
/// **surface** and return the vertex of the cell it first hits.
///
/// The terrain is the flat-shaded height field [`crate::mesh::build_mesh`] draws
/// — two triangles per grid cell. This walks those same triangles, keeps the
/// **nearest** front hit (smallest positive distance along the ray, so a near
/// ridge correctly *occludes* the land behind it), and snaps that hit point to
/// the closest of the struck cell's four corners — the grid vertex under the
/// cursor, the one #9 raises/lowers and the readout reports. `vertical_scale`
/// matches the mesh's, so the surface picked is exactly the surface drawn
/// ([`crate::mesh::vertex_position`]).
///
/// Returns `None` when the ray meets no triangle at all — an empty or single-row
/// frame, or a cursor pointing past the land into the background. Unlike a
/// nearest-to-the-ray-*line* pick, a miss is a real miss, not the closest stray
/// vertex, and an occluded vertex is never returned.
#[must_use]
pub fn pick_vertex(
    ray: &Ray,
    frame: &TerrainFrame<'_>,
    vertical_scale: f32,
) -> Option<PickedVertex> {
    let (width, depth) = (frame.width(), frame.height());
    let mut best: Option<(f32, PickedVertex)> = None; // (distance along ray, vertex)

    for y in 0..depth.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let (Some(h00), Some(h10), Some(h01), Some(h11)) = (
                frame.get(x, y),
                frame.get(x + 1, y),
                frame.get(x, y + 1),
                frame.get(x + 1, y + 1),
            ) else {
                continue; // a well-formed frame skips no cell
            };
            // The cell's four corners, positioned exactly as the mesh draws them.
            let corners = [
                (x, y, h00),
                (x + 1, y, h10),
                (x, y + 1, h01),
                (x + 1, y + 1, h11),
            ]
            .map(|(cx, cy, h)| Corner {
                x: cx,
                y: cy,
                height: h,
                position: vertex_position(cx, cy, h, width, depth, vertical_scale),
            });

            // The two triangles `build_mesh` splits this cell into (same c00–c11
            // diagonal), so the picked surface is exactly the surface drawn.
            for triangle in [
                [corners[0], corners[1], corners[3]],
                [corners[0], corners[3], corners[2]],
            ] {
                let Some(t) = ray_triangle(
                    ray,
                    triangle[0].position,
                    triangle[1].position,
                    triangle[2].position,
                ) else {
                    continue;
                };
                let take = match best {
                    None => true,
                    Some((best_t, _)) => t < best_t,
                };
                if take {
                    best = Some((t, nearest_corner(point_at(ray, t), &corners)));
                }
            }
        }
    }

    best.map(|(_, vertex)| vertex)
}

/// A cell corner during picking: its grid coordinate and height plus the world
/// position the mesh draws it at, so the struck cell can be snapped to whichever
/// corner the cursor points nearest.
#[derive(Clone, Copy)]
struct Corner {
    x: u32,
    y: u32,
    height: i32,
    position: Vec3,
}

/// The world point `t` units along `ray` from its origin — where the ray meets
/// the surface.
fn point_at(ray: &Ray, t: f32) -> Vec3 {
    [
        ray.origin[0] + ray.direction[0] * t,
        ray.origin[1] + ray.direction[1] * t,
        ray.origin[2] + ray.direction[2] * t,
    ]
}

/// The corner of `corners` nearest `hit` in world space — the grid vertex the
/// cursor points at within the struck cell. Ties break toward the earlier corner
/// (row-major: `(x, y)`, then `(x+1, y)`, then the next row), a stable documented
/// order rather than an arbitrary one.
fn nearest_corner(hit: Vec3, corners: &[Corner; 4]) -> PickedVertex {
    let mut best = corners[0];
    let mut best_dist_sq = f32::INFINITY;
    for &corner in corners {
        let delta = sub(corner.position, hit);
        let dist_sq = dot(delta, delta);
        if dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            best = corner;
        }
    }
    PickedVertex {
        x: best.x,
        y: best.y,
        height: best.height,
    }
}

/// Möller–Trumbore ray/triangle intersection: the positive distance along `ray`
/// to triangle `p0→p1→p2`, or `None` if the ray misses it, runs parallel to it,
/// or meets it at/behind the origin. **Double-sided** — the terrain's triangle
/// winding varies, and picking wants the surface hit regardless of facing. The
/// small epsilon makes shared edges and vertices belong to at least one adjacent
/// triangle, so a ray through a seam still resolves.
fn ray_triangle(ray: &Ray, p0: Vec3, p1: Vec3, p2: Vec3) -> Option<f32> {
    // Structural geometry tolerance for the parallel test and edge inclusivity —
    // not a tunable, like `normalize`'s zero-length guard.
    const EPS: f32 = 1e-6;
    let edge1 = sub(p1, p0);
    let edge2 = sub(p2, p0);
    let pvec = cross(ray.direction, edge2);
    let det = dot(edge1, pvec);
    if det.abs() < EPS {
        return None; // the ray is parallel to the triangle's plane
    }
    let inv_det = 1.0 / det;
    let tvec = sub(ray.origin, p0);
    let bary_u = dot(tvec, pvec) * inv_det;
    if !(-EPS..=1.0 + EPS).contains(&bary_u) {
        return None;
    }
    let qvec = cross(tvec, edge1);
    let bary_v = dot(ray.direction, qvec) * inv_det;
    if bary_v < -EPS || bary_u + bary_v > 1.0 + EPS {
        return None;
    }
    let hit_distance = dot(edge2, qvec) * inv_det;
    if hit_distance > EPS {
        Some(hit_distance)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{PickedVertex, Ray, cursor_ndc, pick_vertex, reticle_ray, screen_ray};
    use crate::camera::Camera;
    use crate::math::{dot, normalize, sub};
    use providence_ports::TerrainFrame;

    /// A camera looking straight down the world −y axis at the origin, from
    /// height `eye_y`. `up` is horizontal (as it must be for a top-down view).
    fn top_down(eye_y: f32) -> Camera {
        Camera {
            eye: [0.0, eye_y, 0.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 0.0, -1.0],
            fov_y_radians: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 1000.0,
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    #[test]
    fn the_reticle_ray_points_from_the_eye_along_the_view_direction() {
        let camera = top_down(20.0);
        let ray = reticle_ray(&camera, 16.0 / 9.0);
        assert!(
            ray.origin
                .iter()
                .zip(camera.eye)
                .all(|(a, b)| approx(*a, b)),
            "the ray starts at the eye",
        );
        let forward = normalize(sub(camera.target, camera.eye));
        assert!(
            approx(ray.direction[0], forward[0])
                && approx(ray.direction[1], forward[1])
                && approx(ray.direction[2], forward[2]),
            "the centre ray is the view direction",
        );
        assert!(approx(dot(ray.direction, ray.direction), 1.0), "unit dir");
    }

    #[test]
    fn an_off_centre_ndc_tilts_the_ray_toward_that_side() {
        let camera = top_down(20.0);
        // Looking down −y with up = −z, the world +x axis is the camera's right.
        // An ndc to the right should tip the ray in +x.
        let right_ray = screen_ray(&camera, 1.0, [1.0, 0.0]);
        assert!(
            right_ray.direction[0] > 0.0,
            "a right-of-centre ray leans toward +x",
        );
        assert!(approx(dot(right_ray.direction, right_ray.direction), 1.0));
    }

    #[test]
    fn the_reticle_picks_the_vertex_under_the_crosshair() {
        // 3×3 grid, flat but for a raised centre. Looking straight down the
        // y-axis, the centre vertex sits exactly on the ray, so it is picked.
        let mut heights = [0; 9];
        heights[4] = 5; // centre vertex (1, 1)
        let frame = TerrainFrame::new(3, 3, &heights, &[], 0);
        let ray = reticle_ray(&top_down(20.0), 1.0);
        assert_eq!(
            pick_vertex(&ray, &frame, 1.0),
            Some(PickedVertex {
                x: 1,
                y: 1,
                height: 5
            }),
        );
    }

    #[test]
    fn nothing_in_front_of_the_camera_picks_nothing() {
        // Camera below the terrain looking further down: every vertex is behind
        // the ray, so there is nothing to pick.
        let camera = Camera {
            eye: [0.0, -50.0, 0.0],
            target: [0.0, -100.0, 0.0],
            up: [0.0, 0.0, -1.0],
            fov_y_radians: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 1000.0,
        };
        let heights = [0; 9];
        let frame = TerrainFrame::new(3, 3, &heights, &[], 0);
        let ray = reticle_ray(&camera, 1.0);
        assert_eq!(pick_vertex(&ray, &frame, 1.0), None);
    }

    #[test]
    fn an_empty_frame_has_no_pick() {
        let frame = TerrainFrame::new(0, 0, &[], &[], 0);
        let ray = reticle_ray(&top_down(20.0), 1.0);
        assert_eq!(pick_vertex(&ray, &frame, 1.0), None);
    }

    #[test]
    fn the_screen_centre_pixel_maps_to_the_ndc_origin() {
        // A cursor at the middle of an 800×600 surface is the reticle ([0, 0]),
        // so cursor picking through the centre agrees with reticle_ray.
        let ndc = cursor_ndc((400.0, 300.0), (800, 600));
        assert!(
            approx(ndc[0], 0.0) && approx(ndc[1], 0.0),
            "centre → origin"
        );
    }

    #[test]
    fn cursor_ndc_flips_y_and_spans_the_corners() {
        // Top-left pixel → NDC (-1, +1); bottom-right → (+1, -1) (y is flipped
        // because pixels grow downward while NDC grows upward).
        let top_left = cursor_ndc((0.0, 0.0), (800, 600));
        assert!(approx(top_left[0], -1.0) && approx(top_left[1], 1.0));
        let bottom_right = cursor_ndc((800.0, 600.0), (800, 600));
        assert!(approx(bottom_right[0], 1.0) && approx(bottom_right[1], -1.0));
    }

    #[test]
    fn a_zero_sized_surface_maps_to_the_centre() {
        let ndc = cursor_ndc((10.0, 10.0), (0, 0));
        assert!(
            approx(ndc[0], 0.0) && approx(ndc[1], 0.0),
            "degenerate → centre"
        );
    }

    #[test]
    fn a_cursor_left_of_centre_picks_a_left_column() {
        // Flat 5×5 land spanning world x ∈ [−2, 2]. Looking straight down with
        // up = −z, world +x is the camera's right, so a cursor a little left of
        // centre casts a ray striking the ground left of the middle column — the
        // picked vertex is a left one (the cursor-tracked pick #9 needs), now via
        // a true surface hit rather than nearest-to-the-ray-line.
        let heights = [0; 25];
        let frame = TerrainFrame::new(5, 5, &heights, &[], 0);
        let camera = top_down(12.0);
        let ndc = cursor_ndc((250.0, 300.0), (600, 600)); // left of the 600-wide centre
        let ray = screen_ray(&camera, 1.0, ndc);
        let picked = pick_vertex(&ray, &frame, 1.0).expect("the cursor is over the land");
        assert!(picked.x < 2, "a left-of-centre cursor picks a left column");
    }

    #[test]
    fn a_cursor_off_the_terrain_hits_nothing() {
        // A flat 3×3 island spanning world x, z ∈ [−1, 1]. A cursor hard against
        // the edge casts a ray meeting the ground plane well outside the meshed
        // cells, so there is no triangle to strike — a miss is a real miss. The
        // old nearest-to-the-ray-line pick always returned *some* vertex here;
        // the surface raycast correctly returns nothing.
        let heights = [0; 9];
        let frame = TerrainFrame::new(3, 3, &heights, &[], 0);
        let camera = top_down(20.0);
        let ndc = cursor_ndc((20.0, 300.0), (800, 600)); // hard against the left edge
        let ray = screen_ray(&camera, 800.0 / 600.0, ndc);
        assert_eq!(
            pick_vertex(&ray, &frame, 1.0),
            None,
            "pointing off the land picks nothing"
        );
    }

    #[test]
    fn the_nearer_surface_occludes_the_land_behind_it() {
        // A 2-wide strip, 4 rows deep: two peaks (rows 1 and 3) with a valley
        // (row 2) between them. A ray skimming in from the near side at the peak's
        // mid-height strikes the near peak first; the valley and the far peak
        // behind it are hidden. A nearest-to-the-ray-line pick could return the
        // occluded valley — the surface raycast never does.
        //                 row y:  0   1   2   3
        //          height (both cols): 0   6   0   6
        let heights = [0, 0, 6, 6, 0, 0, 6, 6]; // row-major, width 2
        let frame = TerrainFrame::new(2, 4, &heights, &[], 0);
        // Horizontal ray at the peak's mid-height, entering from −z (the near
        // side), offset in x so it sits squarely in the single column of cells.
        let ray = Ray {
            origin: [-0.3, 3.0, -6.0],
            direction: [0.0, 0.0, 1.0],
        };
        let picked = pick_vertex(&ray, &frame, 1.0).expect("the ray meets the near peak");
        assert!(
            picked.y <= 1,
            "picked the struck near peak (rows 0–1), not the occluded valley (row 2) or far peak (row 3)"
        );
    }
}
