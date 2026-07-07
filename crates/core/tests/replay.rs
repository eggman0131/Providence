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
//! step loop. It further pins **worldgen** (issue #7 Phase 2, ADR 0021): a
//! fixed seed + params fingerprints to [`GOLDEN_WORLDGEN`], so "same seed ⇒ same
//! world" (I3) is guarded against silent generator drift.

use providence_config::{
    ContentParams, EconomyParams, ManaMode, ManaParams, MountainContent, OpponentParams, Params,
    PlaceholderParams, RaiseParams, RockContent, Shape, ShoreContent, SimParams, TerrainContent,
    TerrainParams, TreeContent, WinLossParams, WorldgenParams,
};
use providence_core::hash::Fnv1a64;
use providence_core::rng::SplitMix64;
use providence_core::state::{State, step};
use providence_core::terrain::{
    Feature, FeatureMap, HeightField, World, generate, lower, place_features, raise,
};
use providence_ports::TerrainCommand;

const SEED: u64 = 0xD1CE;
const STEPS: u64 = 1_000;

/// Committed golden fingerprint of the full state history for
/// (`SEED`, `STEPS`, the fixture params below). Recompute ONLY for an
/// intentional core change, and call the change out in the PR.
const GOLDEN: u64 = 0x804F_981D_B5F9_5BC5;

fn fixture_params() -> Params {
    Params {
        sim: SimParams {
            worldgen: worldgen_fixture(),
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
        content: ContentParams {
            terrain: TerrainContent {
                shore: ShoreContent { band: 2 },
                mountain: MountainContent { min_height: 12 },
                tree: TreeContent {
                    density_permille: 120,
                },
                rock: RockContent {
                    density_permille: 200,
                },
            },
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
            Op::Raise => raise(&mut field, x, y, &params, None),
            Op::Lower => lower(&mut field, x, y, &params, None),
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

// ---------------------------------------------------------------------------
// Worldgen determinism (issue #7 Phase 2, ADR 0021).
//
// "Same seed + same params ⇒ the same world, forever" (I3). A fixed
// WorldgenParams generates a field whose full heightmap is fingerprinted; the
// golden guards the noise → mask → band → conform pipeline against silent drift.
// ---------------------------------------------------------------------------

/// The terrain step invariant the generated world must satisfy (ADR 0017); the
/// shipped unit step, which the whole model assumes.
const WORLDGEN_MAX_STEP: u32 = 1;

/// Committed golden fingerprint of the world generated from [`worldgen_fixture`]
/// at [`WORLDGEN_MAX_STEP`]. Like [`GOLDEN`], recompute this ONLY for an
/// intentional change to the generator, and call it out in the PR.
const GOLDEN_WORLDGEN: u64 = 0xB61C_5EB9_7DB0_298B;

/// A small, fixed island for the golden: an odd, non-square grid so a scaling
/// or indexing slip perturbs the fingerprint, with mixed relief.
fn worldgen_fixture() -> WorldgenParams {
    WorldgenParams {
        width: 24,
        height: 20,
        seed: 0xC0FF_EE99,
        sea_level: 0,
        land_percent: 55,
        shape: Shape::Island,
        relief: 10,
        feature_size: 8,
        detail: 3,
    }
}

/// Fingerprint the whole generated field (dimensions + row-major heights).
fn worldgen_fingerprint() -> u64 {
    let field = generate(&worldgen_fixture(), WORLDGEN_MAX_STEP);
    let mut hasher = Fnv1a64::new();
    hasher.write_u64(u64::from(field.width()));
    hasher.write_u64(u64::from(field.height()));
    for gy in 0..field.height() {
        for gx in 0..field.width() {
            let cell = field.get(gx, gy).expect("in-bounds cell is readable");
            // Fixed little-endian bit pattern of the signed height — stable
            // across platforms, no lossy cast (mirrors the terrain golden).
            hasher.write_u64(u64::from(u32::from_le_bytes(cell.to_le_bytes())));
        }
    }
    hasher.finish()
}

#[test]
fn worldgen_is_deterministic() {
    assert_eq!(
        worldgen_fingerprint(),
        worldgen_fingerprint(),
        "two generations from the same seed + params diverged (I3 violation)"
    );
}

#[test]
fn worldgen_matches_committed_golden() {
    assert_eq!(
        worldgen_fingerprint(),
        GOLDEN_WORLDGEN,
        "worldgen diverged from the committed golden; if this change to the \
         generator is intentional, update GOLDEN_WORLDGEN and say so in the PR"
    );
}

#[test]
fn worldgen_field_satisfies_the_step_invariant() {
    let field = generate(&worldgen_fixture(), WORLDGEN_MAX_STEP);
    assert!(
        field.satisfies_step_invariant(WORLDGEN_MAX_STEP),
        "the generator must hand back an invariant-valid field (ADR 0021 §3)"
    );
}

// ---------------------------------------------------------------------------
// Immovable-feature placement determinism (issue #7 Phase 3, ADR 0017 §5).
//
// "Same seed + params ⇒ the same immovables." A fixed content catalogue over
// the fixture world fingerprints to GOLDEN_FEATURES, guarding seeded placement.
// ---------------------------------------------------------------------------

/// Committed golden fingerprint of the immovables placed on [`worldgen_fixture`]
/// with [`content_fixture`]. Recompute ONLY for an intentional change to
/// placement, and call it out in the PR.
const GOLDEN_FEATURES: u64 = 0x8047_875C_1251_BD26;

/// A terrain content catalogue for the placement golden: dense enough that both
/// trees (on land) and rock (on the peak) actually appear.
fn content_fixture() -> TerrainContent {
    TerrainContent {
        shore: ShoreContent { band: 2 },
        mountain: MountainContent { min_height: 6 },
        tree: TreeContent {
            density_permille: 250,
        },
        rock: RockContent {
            density_permille: 400,
        },
    }
}

/// Fingerprint the placed features (bare / tree / rock per vertex, row-major).
fn features_fingerprint() -> u64 {
    let field = generate(&worldgen_fixture(), WORLDGEN_MAX_STEP);
    let features = place_features(&field, &worldgen_fixture(), &content_fixture());
    let mut hasher = Fnv1a64::new();
    for gy in 0..features.height() {
        for gx in 0..features.width() {
            let code = match features.get(gx, gy) {
                None => 0,
                Some(Feature::Tree) => 1,
                Some(Feature::Rock) => 2,
            };
            hasher.write_u64(code);
        }
    }
    hasher.finish()
}

#[test]
fn feature_placement_is_deterministic() {
    assert_eq!(
        features_fingerprint(),
        features_fingerprint(),
        "two placements from the same seed + content diverged (I3 violation)"
    );
}

#[test]
fn feature_placement_matches_committed_golden() {
    assert_eq!(
        features_fingerprint(),
        GOLDEN_FEATURES,
        "feature placement diverged from the committed golden; if intentional, \
         update GOLDEN_FEATURES and say so in the PR"
    );
}

// ---------------------------------------------------------------------------
// Recorded terrain-command session determinism (issues #9/#10, ADR 0022).
//
// A live, mutating sim must still replay bit-for-bit (I3). A World generated
// from a fixed seed + params, driven through World::apply by a fixed
// TerrainCommand script (a scripted sculpt), fingerprints its heights + the
// vertices moved after each command to GOLDEN_COMMAND_SESSION — the concrete
// I3 coverage for the interactive shaping seam (issue #10's deliverable). The
// script deliberately targets an immovable vertex, so the refuse-and-roll-back
// path (ADR 0017 §5) is part of the recorded, replayed history.
// ---------------------------------------------------------------------------

/// Committed golden fingerprint of the recorded command session
/// ([`command_session_script`] over [`worldgen_fixture`] + [`content_fixture`]).
/// Like [`GOLDEN`], recompute this ONLY for an intentional change to worldgen,
/// placement, the cascade, or the command-apply path — and call it out in the PR.
const GOLDEN_COMMAND_SESSION: u64 = 0x4F24_91F7_6C9F_DEEB;

/// Terrain params for the session golden: the shipped unit step, a ceiling high
/// enough that the centre cone grows over the island's relief, unit cost so
/// `moved` and `cost` track one-to-one.
fn command_session_terrain() -> TerrainParams {
    TerrainParams {
        max_step: 1,
        max_height: 32,
        raise: RaiseParams { mana_cost: 1 },
    }
}

/// The first immovable vertex in `features`, row-major — a deterministic pick
/// (same seed ⇒ same placement) the script targets so the refusal path fires.
fn first_immovable(features: &FeatureMap) -> Option<(u32, u32)> {
    for y in 0..features.height() {
        for x in 0..features.width() {
            if features.is_immovable(x, y) {
                return Some((x, y));
            }
        }
    }
    None
}

/// The first interior vertex whose height *and* all four orthogonal neighbours
/// sit exactly at `sea_level` — a flat patch of sea floor (worldgen pins water
/// flat at the datum, ADR 0021). Row-major and deterministic. Sculpting here
/// behaves exactly like flat ground (predictable cascade) and can never be
/// refused: water carries no immovables (ADR 0017 §5).
fn flat_sea_patch(field: &HeightField, sea_level: i32) -> Option<(u32, u32)> {
    for y in 1..field.height().saturating_sub(1) {
        for x in 1..field.width().saturating_sub(1) {
            let flat = [(x, y), (x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .all(|&(nx, ny)| field.get(nx, ny) == Some(sea_level));
            if flat {
                return Some((x, y));
            }
        }
    }
    None
}

/// The scripted sculpt over the fixture world: three raises on a flat sea patch
/// grow a stepped cone, two lowers reverse part of it, then a raise/lower on a
/// known immovable vertex is refused (moved 0) — so the fingerprint covers a
/// real cascade *and* the roll-back path (ADR 0017 §5). A fixed, deterministic
/// sequence (same seed ⇒ same field + placement ⇒ same script).
fn command_session_script(
    field: &HeightField,
    features: &FeatureMap,
    sea_level: i32,
) -> Vec<TerrainCommand> {
    let (sx, sy) = flat_sea_patch(field, sea_level).expect("the island fixture has open sea");
    let mut script = vec![
        TerrainCommand::Raise { x: sx, y: sy },
        TerrainCommand::Raise { x: sx, y: sy },
        TerrainCommand::Raise { x: sx, y: sy },
        TerrainCommand::Lower { x: sx, y: sy },
        TerrainCommand::Lower { x: sx + 1, y: sy },
    ];
    if let Some((ix, iy)) = first_immovable(features) {
        script.push(TerrainCommand::Raise { x: ix, y: iy });
        script.push(TerrainCommand::Lower { x: ix, y: iy });
    }
    script
}

/// Fingerprint the whole field (row-major heights) plus the `moved` count after
/// each command in the scripted session — the command-seam analogue of the
/// shaping-history fingerprint.
fn command_session_fingerprint() -> u64 {
    let worldgen = worldgen_fixture();
    let terrain = command_session_terrain();
    let field = generate(&worldgen, WORLDGEN_MAX_STEP);
    let features = place_features(&field, &worldgen, &content_fixture());
    let script = command_session_script(&field, &features, worldgen.sea_level);
    let mut world = World::new(field, Some(features));

    let mut hasher = Fnv1a64::new();
    for command in script {
        let outcome = world.apply(&terrain, command);
        hasher.write_u64(u64::from(outcome.moved));
        for gy in 0..world.height() {
            for gx in 0..world.width() {
                let cell = world
                    .field()
                    .get(gx, gy)
                    .expect("in-bounds cell is readable");
                // Fixed little-endian bit pattern of the signed height — stable
                // across platforms, no lossy cast (mirrors the terrain golden).
                hasher.write_u64(u64::from(u32::from_le_bytes(cell.to_le_bytes())));
            }
        }
    }
    hasher.finish()
}

#[test]
fn command_session_is_deterministic() {
    assert_eq!(
        command_session_fingerprint(),
        command_session_fingerprint(),
        "two runs of the same recorded command session diverged (I3 violation)"
    );
}

#[test]
fn command_session_matches_committed_golden() {
    assert_eq!(
        command_session_fingerprint(),
        GOLDEN_COMMAND_SESSION,
        "the recorded command session diverged from the committed golden; if this \
         change to worldgen/placement/cascade/apply is intentional, update \
         GOLDEN_COMMAND_SESSION and say so in the PR"
    );
}

#[test]
fn command_session_exercises_immovable_refusal() {
    // Guards the intent of the script: the fixture world *has* immovables and
    // the session actually targets one, so the refuse-and-roll-back path is
    // covered (not silently skipped if placement shifts). A raise on the first
    // immovable moves nothing and leaves the field untouched (ADR 0017 §5).
    let worldgen = worldgen_fixture();
    let field = generate(&worldgen, WORLDGEN_MAX_STEP);
    let features = place_features(&field, &worldgen, &content_fixture());
    let (ix, iy) = first_immovable(&features).expect("the dense fixture places immovables");

    let mut world = World::new(field, Some(features));
    let before = world.field().clone();
    let outcome = world.apply(
        &command_session_terrain(),
        TerrainCommand::Raise { x: ix, y: iy },
    );
    assert_eq!(outcome.moved, 0, "raising an immovable vertex is refused");
    assert_eq!(
        world.field(),
        &before,
        "the refused op leaves the field untouched"
    );
}
