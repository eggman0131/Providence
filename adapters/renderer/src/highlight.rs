//! The hover-highlight glow shape (issue #12).
//!
//! Pure and GPU-free. Defines the soft radial falloff the terrain shader paints
//! on the surface under the cursor to mark the vertex a shaping click would
//! target. The falloff is kept here — and unit-tested — so the *shape* of the
//! glow is pinned without a GPU (I9): full at the hovered vertex, easing smoothly
//! to nothing at its rim, and a flat zero beyond, so it reads as a gentle pool of
//! light rather than a hard-edged dot or ring. The WGSL in [`crate::gpu`] mirrors
//! this maths exactly; this is its canonical, testable definition.
//!
//! Like the camera and the shaping tween, the highlight is presentation only and
//! adapter-local (ADR 0020 §3): the renderer resolves the cursor-picked vertex at
//! the edge and tints the surface around it; nothing here reaches the core.

/// The glow's peak-normalised strength at horizontal `distance` from the hovered
/// vertex, over a soft disc of the given `radius` (same world units).
///
/// `1.0` at the centre, eased to `0.0` at the rim with zero slope at both ends (a
/// smoothstep), and a flat `0.0` beyond the rim — so the highlight is a gentle
/// pool of light, never a ring or a hard dot. A non-positive `radius` disables
/// the glow (returns `0.0`). The caller scales the result by
/// `render.highlight.intensity` and adds it, tinted, to the lit surface colour.
#[must_use]
pub fn glow_falloff(distance: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    // Smoothstep from the rim inward: `s` is 1 at the centre and 0 at the rim.
    let s = (1.0 - distance / radius).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

#[cfg(test)]
mod tests {
    use super::glow_falloff;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-6
    }

    #[test]
    fn the_glow_is_full_at_the_hovered_vertex() {
        assert!(
            approx(glow_falloff(0.0, 2.5), 1.0),
            "dead centre glows at full strength"
        );
    }

    #[test]
    fn the_glow_fades_to_nothing_at_the_rim() {
        assert!(
            approx(glow_falloff(2.5, 2.5), 0.0),
            "the glow reaches zero exactly at the disc rim"
        );
    }

    #[test]
    fn the_glow_stays_zero_beyond_the_rim() {
        assert!(
            approx(glow_falloff(4.0, 2.5), 0.0),
            "past the rim there is no glow — a soft disc, not an endless wash"
        );
    }

    #[test]
    fn the_glow_eases_monotonically_inward() {
        // A smooth pool of light: strictly brighter the nearer the centre, so it
        // never reads as a ring (which would brighten then dim outward-in).
        let near = glow_falloff(0.5, 2.5);
        let mid = glow_falloff(1.25, 2.5);
        let far = glow_falloff(2.0, 2.5);
        assert!(near > mid && mid > far, "brighter toward the centre");
        assert!(far > 0.0, "still faintly lit inside the rim");
    }

    #[test]
    fn a_smoothstep_flattens_at_both_ends() {
        // Zero slope at the centre and rim (the smoothstep signature): the value
        // just inside each end barely differs from the end itself, so there is no
        // hard edge to the light.
        let near_centre = glow_falloff(0.05, 2.5);
        let near_rim = glow_falloff(2.45, 2.5);
        assert!(near_centre > 0.99, "flat and bright at the centre");
        assert!(near_rim < 0.01, "flat and dark at the rim");
    }

    #[test]
    fn a_non_positive_radius_disables_the_glow() {
        assert!(approx(glow_falloff(0.0, 0.0), 0.0), "zero radius: no glow");
        assert!(
            approx(glow_falloff(0.0, -1.0), 0.0),
            "a negative radius disables rather than misbehaves"
        );
    }
}
