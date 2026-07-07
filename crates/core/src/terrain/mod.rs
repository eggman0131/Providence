//! Terrain — the integer vertex height field and its shaping operations
//! (ADR 0017). The land is the game's primary substrate, built to real depth
//! first (ADR 0019).
//!
//! Issue #6 built the state and shaping across three phases: Phase 1 supplied
//! the state type and the *step invariant* predicate every terrain operation
//! must preserve; Phase 2 added [`raise`] / [`lower`] with the bounded cascade
//! and the moved-vertex cost; Phase 3 pinned both with a randomised invariant
//! property test (`tests/terrain_invariant.rs`) and a terrain determinism
//! golden (`tests/replay.rs`, I3).
//!
//! Issue #7 builds worldgen on top ([ADR 0021](../../../docs/decisions/0021-seeded-parameterised-worldgen.md)):
//! Phase 1 derives terrain *type* and *buildability* from height and sea level
//! ([`derive`]); Phase 2 adds the seeded, parameterised generator; Phase 3
//! realises the immovable-feature seam (ADR 0017 §5).

mod derive;
mod field;
mod shape;

pub use derive::{TerrainType, classify_vertex, is_buildable_face};
pub use field::{Height, HeightField};
pub use shape::{ShapeOutcome, lower, raise};
