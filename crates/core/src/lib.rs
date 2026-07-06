//! The deterministic simulation core (docs/20-architecture.md §2.2).
//!
//! Pure and reproducible (invariant I3): no I/O, no wall-clock, no ambient
//! randomness, no filesystem. `#![no_std]` makes those APIs *unreachable*
//! rather than merely forbidden (ADR 0009). All randomness flows through a
//! seeded [`rng::SplitMix64`] passed in by the caller; parameters arrive as
//! plain data ([`providence_config::Params`]).
//!
//! Phase-1 contents are placeholder scaffolding that proves the gate
//! end-to-end (contract §7.2); real simulation modules land in Phase 3.

#![no_std]
#![forbid(unsafe_code)]

pub mod hash;
pub mod rng;
pub mod state;
