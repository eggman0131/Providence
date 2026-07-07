//! The deterministic simulation core (docs/20-architecture.md §2.2).
//!
//! Pure and reproducible (invariant I3): no I/O, no wall-clock, no ambient
//! randomness, no filesystem. `#![no_std]` makes those APIs *unreachable*
//! rather than merely forbidden (ADR 0009). All randomness flows through a
//! seeded [`rng::SplitMix64`] passed in by the caller; parameters arrive as
//! plain data ([`providence_config::Params`]).
//!
//! Alongside the placeholder scaffolding that proves the gate end-to-end
//! (contract §7.2), the [`terrain`] module holds the first real simulation
//! state — the vertex height field (ADR 0017), built foundation-first
//! (ADR 0019). The core is `#![no_std]` **plus `alloc`** (ADR 0009 §2): the
//! height field is heap-backed, but determinism (I3) fingerprints *heights*,
//! never allocation addresses.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod hash;
pub mod rng;
pub mod state;
pub mod terrain;
