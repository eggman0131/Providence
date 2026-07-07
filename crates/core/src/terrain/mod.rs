//! Terrain — the integer vertex height field and (from Phase 2) its shaping
//! operations (ADR 0017). The land is the game's primary substrate, built to
//! real depth first (ADR 0019).
//!
//! Phase 1 (issue #6) supplies the state type and the *step invariant*
//! predicate every terrain operation must preserve. Raise/lower with the
//! bounded cascade lands in Phase 2; the randomised invariant property test
//! and the replay golden in Phase 3.

mod field;

pub use field::{Height, HeightField};
