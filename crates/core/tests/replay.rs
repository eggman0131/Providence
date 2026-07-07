//! Determinism/replay harness (contract §7.1, invariant I3, ADR 0009).
//!
//! Two runs with identical seed + params must produce a bit-identical state
//! history, and that history's fingerprint must match the committed golden
//! hash. The golden changes only on an intentional, reviewed core change.
//!
//! Alongside the placeholder `step()` history, this also pins the **terrain
//! shaping** model (issue #6 Phase 3, ADR 0017 §3): a fixed raise/lower
//! sequence over a 16×16 field fingerprints to [`GOLDEN_TERRAIN`], guarding the
//! cascade and cost model against silent drift the same way `GOLDEN` guards the
//! step loop.

use providence_config::{
    EconomyParams, ManaMode, ManaParams, OpponentParams, Params, PlaceholderParams, RaiseParams,
    SimParams, TerrainParams, WinLossParams,
};
use providence_core::hash::Fnv1a64;
use providence_core::rng::SplitMix64;
use providence_core::state::{State, step};
use providence_core::terrain::{HeightField, lower, raise};

const SEED: u64 = 0xD1CE;
const STEPS: u64 = 1_000;

/// Committed golden fingerprint of the full state history for
/// (`SEED`, `STEPS`, the fixture params below). Recompute ONLY for an
/// intentional core change, and call the change out in the PR.
const GOLDEN: u64 = 0x804F_981D_B5F9_5BC5;

fn fixture_params() -> Params {
    Params {
        sim: SimParams {
            opponent: OpponentParams { enabled: true },
            economy: EconomyParams {
                mana: ManaParams {
                    mode: ManaMode::Normal,
                },
            },
            winloss: WinLossParams { enabled: true },
            terrain: TerrainParams {
                max_step: 1,
                max_height: 64,
                raise: RaiseParams { mana_cost: 1 },
            },
            placeholder: PlaceholderParams { tick_increment: 1 },
        },
    }
}

/// Run the placeholder simulation and fingerprint every intermediate state.
fn run_history_fingerprint() -> u64 {
    let params = fixture_params();
    let mut rng = SplitMix64::new(SEED);
    let mut state = State::initial();
    let mut hasher = Fnv1a64::new();
    for _ in 0..STEPS {
        state = step(&state, &params, &mut rng);
        hasher.write_u64(state.tick);
        hasher.write_u64(state.accumulator);
    }
    hasher.finish()
}

#[test]
fn identical_inputs_produce_identical_histories() {
    assert_eq!(
        run_history_fingerprint(),
        run_history_fingerprint(),
        "two runs with the same seed + params diverged (I3 violation)"
    );
}

#[test]
fn history_matches_committed_golden() {
    assert_eq!(
        run_history_fingerprint(),
        GOLDEN,
        "state history diverged from the committed golden hash; if this core \
         change is intentional, update GOLDEN and say so in the PR"
    );
}

#[test]
fn params_change_observably_changes_behaviour() {
    // The no-code-change rule (docs/40-parameterisation.md §6.1): a config
    // value change must change observable behaviour with no source edit.
    let mut params = fixture_params();
    params.sim.placeholder.tick_increment = 5;
    let mut rng = SplitMix64::new(SEED);
    let mut state = State::initial();
    for _ in 0..3 {
        state = step(&state, &params, &mut rng);
    }
    assert_eq!(
        state.tick, 15,
        "tick_increment=5 over 3 steps must yield tick 15"
    );
}

// ---------------------------------------------------------------------------
// Terrain shaping determinism (issue #6 Phase 3, ADR 0017 §3).
//
// The step() history above never reads terrain (#6 keeps terrain out of the
// step() seam — that is #10's work), so this is a *separate* golden over the
// pure raise/lower functions: a fixed op sequence over a flat field, its full
// state fingerprinted after each op.
// ---------------------------------------------------------------------------

/// Committed golden fingerprint of the fixed terrain shaping sequence
/// ([`TERRAIN_OPS`]) over a flat 16×16 field. Like [`GOLDEN`], recompute this
/// ONLY for an intentional change to the cascade/cost model, and call the
/// change out in the PR.
const GOLDEN_TERRAIN: u64 = 0xD95B_F22F_FC42_AFA5;

/// Terrain params for the golden: the shipped unit step, a ceiling low enough
/// that the sequence drives the target into it (exercising the clamp), unit
/// cost so `moved` and `cost` track one-to-one.
fn terrain_fixture() -> TerrainParams {
    TerrainParams {
        max_step: 1,
        max_height: 8,
        raise: RaiseParams { mana_cost: 1 },
    }
}

/// One shaping op in the fixed golden sequence.
#[derive(Clone, Copy)]
enum Op {
    Raise,
    Lower,
}

/// A fixed, hand-chosen op sequence: raises at the centre, hard against the
/// west edge, and at the far corner; the centre is then driven into the ceiling
/// (a clamped no-op); finally some lowers. Enough cascade variety — interior,
/// world-edge, ceiling clamp, no-op, and lower — that any change to the shaping
/// model perturbs the fingerprint.
const TERRAIN_OPS: &[(Op, u32, u32)] = &[
    (Op::Raise, 8, 8), // centre
    (Op::Raise, 8, 8), // centre grows a cone
    (Op::Raise, 8, 8),
    (Op::Raise, 0, 4), // hard against the west edge (no wrap)
    (Op::Raise, 0, 4),
    (Op::Raise, 15, 15), // the far corner (two in-grid neighbours)
    (Op::Raise, 8, 8),   // drive the centre toward the ceiling
    (Op::Raise, 8, 8),
    (Op::Raise, 8, 8),
    (Op::Raise, 8, 8),
    (Op::Raise, 8, 8), // centre now at max_height 8
    (Op::Raise, 8, 8), // clamped — a no-op (moved 0)
    (Op::Lower, 8, 8), // ... and back down
    (Op::Lower, 3, 12),
    (Op::Lower, 3, 12),
];

/// Fingerprint the whole field (row-major heights) plus the `moved` count after
/// each op in [`TERRAIN_OPS`] — the shaping analogue of the state-history
/// fingerprint.
fn terrain_history_fingerprint() -> u64 {
    let params = terrain_fixture();
    let mut field = HeightField::flat(16, 16, 0);
    let mut hasher = Fnv1a64::new();
    for &(op, x, y) in TERRAIN_OPS {
        let outcome = match op {
            Op::Raise => raise(&mut field, x, y, &params),
            Op::Lower => lower(&mut field, x, y, &params),
        };
        hasher.write_u64(u64::from(outcome.moved));
        for gy in 0..field.height() {
            for gx in 0..field.width() {
                let cell = field.get(gx, gy).expect("in-bounds cell is readable");
                // Absorb the exact bit pattern of the signed height with a fixed
                // endianness, so the fingerprint is stable across platforms and
                // uses no lossy `as` cast.
                hasher.write_u64(u64::from(u32::from_le_bytes(cell.to_le_bytes())));
            }
        }
    }
    hasher.finish()
}

#[test]
fn terrain_shaping_is_deterministic() {
    assert_eq!(
        terrain_history_fingerprint(),
        terrain_history_fingerprint(),
        "two runs of the same shaping sequence diverged (I3 violation)"
    );
}

#[test]
fn terrain_history_matches_committed_golden() {
    assert_eq!(
        terrain_history_fingerprint(),
        GOLDEN_TERRAIN,
        "terrain shaping diverged from the committed golden; if this change to \
         the cascade/cost model is intentional, update GOLDEN_TERRAIN and say \
         so in the PR"
    );
}
