//! Randomised property test: every shaping op preserves the step invariant
//! (issue #6 Phase 3, ADR 0017 §2).
//!
//! Phase 2's unit tests pin *specific* cascades; this drives *many* random
//! fields through *long* random raise/lower sequences and asserts the step
//! invariant holds after **every single op** — the guarantee the whole terrain
//! model rests on. Randomness comes from the core's own [`SplitMix64`] (zero
//! deps, bit-reproducible), so a failure reproduces exactly from the scenario
//! index printed in the panic.
//!
//! The field starts *flat* (which satisfies the invariant for any `max_step`),
//! so the operation precondition holds on entry to the first op; if every op
//! restores the invariant, it holds inductively on entry to every op after —
//! and each iteration re-checks it, validating that induction step by step.

use providence_config::{RaiseParams, TerrainParams};
use providence_core::rng::SplitMix64;
use providence_core::terrain::{HeightField, lower, raise};

// Test-scale knobs (not shipped behaviour): enough random fields and ops to
// force deep cascades, ceiling clamps, and world-edge halts, while staying a
// fast unit-speed test.
const SCENARIOS: u32 = 400;
const OPS_PER_SCENARIO: u32 = 150;

/// A pseudo-random value in `[0, modulus)` (`modulus > 0`). Reduced in `u64`,
/// then narrowed losslessly: the remainder is `< modulus <= u32::MAX`, so the
/// `try_from` never fails — no lossy `as` cast (clippy-pedantic clean).
fn below(rng: &mut SplitMix64, modulus: u32) -> u32 {
    u32::try_from(rng.next_u64() % u64::from(modulus)).expect("remainder < modulus fits u32")
}

#[test]
fn every_shaping_op_preserves_the_step_invariant() {
    for scenario in 0..SCENARIOS {
        // One deterministic stream per scenario.
        let mut rng = SplitMix64::new(u64::from(scenario));

        let width = 1 + below(&mut rng, 12); // 1..=12
        let height = 1 + below(&mut rng, 12); // 1..=12
        // 1..=3 — prove the cascade generalises past the shipped unit step
        // (ADR 0017 ships max_step = 1).
        let max_step = 1 + below(&mut rng, 3);
        // A low ceiling (0..=8) relative to the op budget so raises actually
        // clamp, exercising the max_height bound.
        let max_height = i32::try_from(below(&mut rng, 9)).expect("value < 9 fits i32");
        // 1..=4 — exercise the cost multiply, not just the moved count.
        let mana_cost = 1 + below(&mut rng, 4);
        let params = TerrainParams {
            max_step,
            max_height,
            raise: RaiseParams { mana_cost },
        };

        let mut field = HeightField::flat(width, height, 0);
        assert!(
            field.satisfies_step_invariant(max_step),
            "a flat field must satisfy the invariant (scenario {scenario})"
        );

        for op in 0..OPS_PER_SCENARIO {
            // A target that is usually in-bounds but occasionally past an edge
            // (`== width` / `== height`) to exercise the out-of-bounds no-op.
            let x = below(&mut rng, width + 1);
            let y = below(&mut rng, height + 1);
            let outcome = if rng.next_u64() & 1 == 0 {
                raise(&mut field, x, y, &params)
            } else {
                lower(&mut field, x, y, &params)
            };

            assert!(
                field.satisfies_step_invariant(max_step),
                "step invariant broke after op {op} at ({x},{y}) in scenario \
                 {scenario} (max_step={max_step}, max_height={max_height})"
            );
            assert_eq!(
                outcome.cost,
                u64::from(outcome.moved) * u64::from(mana_cost),
                "cost must equal moved × mana_cost (scenario {scenario}, op {op})"
            );
        }
    }
}
