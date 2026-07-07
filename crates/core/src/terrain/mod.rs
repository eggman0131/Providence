//! Terrain — the integer vertex height field and its shaping operations
//! (ADR 0017). The land is the game's primary substrate, built to real depth
//! first (ADR 0019).
//!
//! Issue #6 built this across three phases: Phase 1 supplied the state type and
//! the *step invariant* predicate every terrain operation must preserve;
//! Phase 2 added [`raise`] / [`lower`] with the bounded cascade and the
//! moved-vertex cost; Phase 3 pinned both with a randomised invariant property
//! test (`tests/terrain_invariant.rs`) and a terrain determinism golden
//! (`tests/replay.rs`, I3). The immovable-feature seam (ADR 0017 §5) and seeded
//! worldgen arrive with issue #7.

mod field;
mod shape;

pub use field::{Height, HeightField};
pub use shape::{ShapeOutcome, lower, raise};
