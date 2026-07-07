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
//! ([`derive`]); Phase 2 adds the seeded, parameterised generator
//! ([`worldgen`]); Phase 3 realises the immovable-feature seam (ADR 0017 §5) —
//! terrain-owned immovables ([`feature`]) placed by worldgen, which the shaping
//! ops refuse to disturb.
//!
//! Issues #9/#10 add the interactive command seam ([ADR 0022](../../../docs/decisions/0022-interactive-shaping-seam-input-command-simdriver.md)):
//! a [`World`] bundles the field and its immovables and consumes a discrete,
//! recorded [`providence_ports::TerrainCommand`] via [`World::apply`], the core
//! side of the [`SimDriver`](providence_ports::SimDriver) port a live session
//! drives — so shaping the land stays deterministic (fixed integer commands, no
//! wall-clock) even as it happens in real time.

mod derive;
mod feature;
mod field;
mod shape;
mod world;
mod worldgen;

pub use derive::{TerrainType, classify_vertex, is_buildable_face};
pub use feature::{Feature, FeatureMap};
pub use field::{Height, HeightField};
pub use shape::{ShapeOutcome, lower, raise};
pub use world::World;
pub use worldgen::{generate, place_features};
