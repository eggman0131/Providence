//! Placeholder world state and the deterministic `step` transition.
//!
//! Phase-1 scaffolding (contract §7.2): just enough state to exercise the
//! full deterministic pipeline — params in, seeded RNG in, bit-identical
//! state out. Replaced by real world/terrain/population state in Phase 3.

use providence_config::Params;

use crate::rng::SplitMix64;

/// Complete simulation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// Simulation ticks elapsed.
    pub tick: u64,
    /// Accumulated RNG output — exists to exercise the seeded-RNG path.
    pub accumulator: u64,
}

impl State {
    /// The state before any step has been applied.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            tick: 0,
            accumulator: 0,
        }
    }
}

/// Advance the state by one step: `next = step(state, params, rng)`.
///
/// Pure: same inputs ⇒ bit-identical output (invariant I3). The replay
/// harness (`tests/replay.rs`) verifies this against a committed golden
/// fingerprint.
#[must_use]
pub fn step(state: &State, params: &Params, rng: &mut SplitMix64) -> State {
    State {
        tick: state
            .tick
            .wrapping_add(params.sim.placeholder.tick_increment),
        accumulator: state.accumulator.wrapping_add(rng.next_u64()),
    }
}
