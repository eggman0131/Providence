//! Pure input mapping (ADR 0022 §3; issue #9 Phase 2).
//!
//! GPU- and `winit`-free. Turns a resolved mouse gesture into a decision — is
//! this a shaping *click* or a camera *drag*, and if a click, does it raise or
//! lower? — against the `input.shape.*` bindings ([`providence_config::InputParams`]).
//! The window edge ([`crate::window`]) owns the raw `winit` events and the
//! camera; it converts a platform button to a [`providence_config::PointerButton`],
//! accumulates cursor motion, and asks these functions for the verdict, then
//! emits the [`TerrainCommand`] at the picked vertex. Keeping the *decision*
//! here — and free of `winit`/GPU — is what lets the gate unit-test the control
//! scheme without a window (I9).

use providence_config::{PointerButton, ShapeInputParams};
use providence_ports::TerrainCommand;

/// Which way a bound button shapes the picked vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeAction {
    /// Raise the vertex by one step (its cascade rippling outward).
    Raise,
    /// Lower it by one step — the mirror of [`ShapeAction::Raise`].
    Lower,
}

impl ShapeAction {
    /// The discrete [`TerrainCommand`] this action issues at grid `(x, y)` —
    /// the *one* vocabulary the core consumes and the session records (ADR 0022
    /// §1).
    #[must_use]
    pub fn command(self, x: u32, y: u32) -> TerrainCommand {
        match self {
            ShapeAction::Raise => TerrainCommand::Raise { x, y },
            ShapeAction::Lower => TerrainCommand::Lower { x, y },
        }
    }
}

/// Which shaping action `button` is bound to under `shape`, or `None` if it is
/// bound to neither (so the click does nothing but the camera gesture, if any,
/// still stands).
///
/// `raise_button` is tested first, so if a mis-configuration binds the same
/// button to both, raise wins — a stable, documented tie-break rather than a
/// panic.
#[must_use]
pub fn shape_action(button: PointerButton, shape: &ShapeInputParams) -> Option<ShapeAction> {
    if button == shape.raise_button {
        Some(ShapeAction::Raise)
    } else if button == shape.lower_button {
        Some(ShapeAction::Lower)
    } else {
        None
    }
}

/// Was a press→release a shaping *click* rather than a camera *drag*? True iff
/// the cursor moved no more than `shape.click_drag_threshold_px` while the
/// button was held (the Director's ruling, ADR 0022): a still-ish press shapes,
/// a drag orbits/pans. `motion_px` is the accumulated cursor path length, in
/// physical pixels, since the press.
#[must_use]
pub fn is_shaping_click(motion_px: f32, shape: &ShapeInputParams) -> bool {
    motion_px <= shape.click_drag_threshold_px
}

#[cfg(test)]
mod tests {
    use super::{ShapeAction, is_shaping_click, shape_action};
    use providence_config::{PointerButton, ShapeInputParams};
    use providence_ports::TerrainCommand;

    /// The shipped default binding: left raises, right lowers, 6px click slack.
    fn shape() -> ShapeInputParams {
        ShapeInputParams {
            raise_button: PointerButton::Left,
            lower_button: PointerButton::Right,
            click_drag_threshold_px: 6.0,
        }
    }

    #[test]
    fn the_bound_buttons_map_to_raise_and_lower() {
        assert_eq!(
            shape_action(PointerButton::Left, &shape()),
            Some(ShapeAction::Raise),
        );
        assert_eq!(
            shape_action(PointerButton::Right, &shape()),
            Some(ShapeAction::Lower),
        );
    }

    #[test]
    fn an_unbound_button_shapes_nothing() {
        assert_eq!(shape_action(PointerButton::Middle, &shape()), None);
    }

    #[test]
    fn bindings_follow_config_not_hardcoded_buttons() {
        // Swap the bindings: right now raises, left lowers. The mapping tracks
        // config, so the control scheme is genuinely tunable (I1).
        let swapped = ShapeInputParams {
            raise_button: PointerButton::Right,
            lower_button: PointerButton::Left,
            click_drag_threshold_px: 6.0,
        };
        assert_eq!(
            shape_action(PointerButton::Right, &swapped),
            Some(ShapeAction::Raise),
        );
        assert_eq!(
            shape_action(PointerButton::Left, &swapped),
            Some(ShapeAction::Lower),
        );
    }

    #[test]
    fn a_button_bound_to_both_resolves_to_raise() {
        let both = ShapeInputParams {
            raise_button: PointerButton::Left,
            lower_button: PointerButton::Left,
            click_drag_threshold_px: 6.0,
        };
        assert_eq!(
            shape_action(PointerButton::Left, &both),
            Some(ShapeAction::Raise),
            "a doubly-bound button breaks the tie toward raise",
        );
    }

    #[test]
    fn an_action_issues_the_matching_command_at_the_vertex() {
        assert_eq!(
            ShapeAction::Raise.command(3, 7),
            TerrainCommand::Raise { x: 3, y: 7 },
        );
        assert_eq!(
            ShapeAction::Lower.command(3, 7),
            TerrainCommand::Lower { x: 3, y: 7 },
        );
    }

    #[test]
    fn motion_within_the_threshold_is_a_click() {
        assert!(is_shaping_click(0.0, &shape()), "no motion is a click");
        assert!(
            is_shaping_click(6.0, &shape()),
            "exactly the threshold is a click"
        );
    }

    #[test]
    fn motion_beyond_the_threshold_is_a_drag() {
        assert!(
            !is_shaping_click(6.01, &shape()),
            "past the threshold is a drag, not a click",
        );
    }

    #[test]
    fn a_zero_threshold_makes_any_motion_a_drag() {
        let strict = ShapeInputParams {
            click_drag_threshold_px: 0.0,
            ..shape()
        };
        assert!(
            is_shaping_click(0.0, &strict),
            "a dead-still press still clicks"
        );
        assert!(
            !is_shaping_click(0.5, &strict),
            "the faintest drift is a drag"
        );
    }
}
