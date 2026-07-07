//! Application layer — orchestration only, no game rules
//! (docs/20-architecture.md §2.3).
//!
//! Two sessions live here, deliberately *not* entangled:
//! - [`Session`] — the Phase-1 placeholder: own params + seed + state and
//!   advance it through the core's `step` (the scaffold that proved the gate).
//! - [`WorkbenchSession`] — the interactive terrain session (ADR 0022 §3): the
//!   application side of the workbench seam. It owns a core [`World`], applies a
//!   discrete [`TerrainCommand`] on [`submit`](providence_ports::SimDriver::submit),
//!   and records the command log — so a session is `seed + params + log` and
//!   replays bit-for-bit (I3). It implements [`SimDriver`], the port the
//!   renderer holds; input reaches the sim only through it.
//!
//! The turn scheduler and full port mediation still land in later phases
//! (contract §7.4).

#![forbid(unsafe_code)]

use providence_config::{Params, TerrainParams};
use providence_core::rng::SplitMix64;
use providence_core::state::{State, step};
use providence_core::terrain::{World, generate, place_features};
use providence_ports::{Height, SimDriver, TerrainCommand};

/// A running game session: current state plus the config and seed it runs
/// under (docs/20-architecture.md §2.3).
#[derive(Debug)]
pub struct Session {
    params: Params,
    rng: SplitMix64,
    state: State,
}

impl Session {
    /// Start a session from validated params and a seed.
    #[must_use]
    pub fn new(params: Params, seed: u64) -> Self {
        Self {
            params,
            rng: SplitMix64::new(seed),
            state: State::initial(),
        }
    }

    /// Advance the simulation by one step.
    pub fn advance(&mut self) {
        self.state = step(&self.state, &self.params, &mut self.rng);
    }

    /// Current state (read-only).
    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }
}

/// An interactive terrain-shaping session (ADR 0022 §3) — the application side
/// of the workbench seam.
///
/// It owns the core [`World`], the [`TerrainParams`] that price shaping, a
/// logical tick counter, a render revision, and the recorded command log. A
/// session is therefore exactly *seed + params + log*: replaying the log against
/// a fresh world from the same params reproduces the field bit-for-bit (I3). It
/// implements [`SimDriver`], the port the renderer holds — input arrives only
/// through [`submit`](SimDriver::submit), as a discrete [`TerrainCommand`].
///
/// A *new* type alongside the placeholder [`Session`], not entangled with it:
/// `Session` advances the placeholder `step` loop; `WorkbenchSession` shapes
/// terrain.
#[derive(Debug)]
pub struct WorkbenchSession {
    world: World,
    terrain: TerrainParams,
    tick: u64,
    revision: u64,
    log: Vec<(u64, TerrainCommand)>,
}

impl WorkbenchSession {
    /// Build a session over a freshly generated world (ADR 0021): the seed and
    /// `sim.worldgen.*` generate the height field, `content.terrain.*` scatter
    /// its immovables — exactly as the workbench composition root does.
    #[must_use]
    pub fn new(params: &Params) -> Self {
        let field = generate(&params.sim.worldgen, params.sim.terrain.max_step);
        let features = place_features(&field, &params.sim.worldgen, &params.content.terrain);
        Self {
            world: World::new(field, Some(features)),
            terrain: params.sim.terrain.clone(),
            tick: 0,
            revision: 0,
            log: Vec::new(),
        }
    }

    /// The current logical tick — one per submitted command (ADR 0022 §5).
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// The recorded `(tick, command)` log — the transcript a replay re-applies.
    #[must_use]
    pub fn log(&self) -> &[(u64, TerrainCommand)] {
        &self.log
    }

    /// The world (read-only) — for the composition-root census and replay checks.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }
}

impl SimDriver for WorkbenchSession {
    fn submit(&mut self, command: TerrainCommand) {
        // Apply on submit (ADR 0022 §5): terrain has no per-tick evolution yet,
        // so each command applies and advances the logical tick by one. The log
        // records every submission, so it is a faithful, replayable transcript.
        let outcome = self.world.apply(&self.terrain, command);
        self.log.push((self.tick, command));
        self.tick += 1;
        // Revision tracks *visible* change: bump only when the heights actually
        // moved, so the renderer animates real cascades and ignores no-ops
        // (out-of-bounds, ceiling clamp, or an immovable refusal). ADR 0022 §3.
        if outcome.moved > 0 {
            self.revision += 1;
        }
    }

    fn width(&self) -> u32 {
        self.world.width()
    }

    fn height(&self) -> u32 {
        self.world.height()
    }

    fn heights(&self) -> &[Height] {
        self.world.heights()
    }

    fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use providence_config::{
        ContentParams, EconomyParams, ManaMode, ManaParams, MountainContent, OpponentParams,
        Params, PlaceholderParams, RaiseParams, RockContent, Shape, ShoreContent, SimParams,
        TerrainContent, TerrainParams, TreeContent, WinLossParams, WorldgenParams,
    };

    use super::{Session, WorkbenchSession};
    use providence_core::terrain::{World, generate, place_features};
    use providence_ports::{SimDriver, TerrainCommand};

    fn params() -> Params {
        Params {
            sim: SimParams {
                worldgen: WorldgenParams {
                    width: 32,
                    height: 32,
                    seed: 1,
                    sea_level: 0,
                    land_percent: 55,
                    shape: Shape::Island,
                    relief: 12,
                    feature_size: 16,
                    detail: 3,
                },
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

    #[test]
    fn sessions_with_identical_inputs_stay_identical() {
        let mut a = Session::new(params(), 42);
        let mut b = Session::new(params(), 42);
        for _ in 0..100 {
            a.advance();
            b.advance();
        }
        assert_eq!(
            a.state(),
            b.state(),
            "same seed + params must stay bit-identical (I3)"
        );
    }

    #[test]
    fn advancing_moves_the_tick_by_the_configured_increment() {
        let mut session = Session::new(params(), 42);
        session.advance();
        session.advance();
        assert_eq!(session.state().tick, 2);
    }

    // --- WorkbenchSession: the interactive command seam (ADR 0022) ------------

    /// A guaranteed-shapeable vertex of the fixture world: the first interior
    /// vertex whose height and all four orthogonal neighbours sit at `sea_level`
    /// — a flat patch of sea floor. Water carries no immovables (ADR 0017 §5), so
    /// a raise here always moves the target (never refused), unlike the island's
    /// rock-topped centre. Deterministic (same seed ⇒ same field).
    fn flat_sea_vertex(session: &WorkbenchSession, params: &Params) -> (u32, u32) {
        let sea = params.sim.worldgen.sea_level;
        let (w, h) = (session.width(), session.height());
        let heights = session.heights();
        let at = |x: u32, y: u32| heights[(y * w + x) as usize];
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                if [(x, y), (x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                    .iter()
                    .all(|&(nx, ny)| at(nx, ny) == sea)
                {
                    return (x, y);
                }
            }
        }
        panic!("the island fixture must have an open-sea patch to sculpt");
    }

    #[test]
    fn new_workbench_session_starts_empty_over_the_generated_world() {
        let params = params();
        let session = WorkbenchSession::new(&params);
        assert_eq!(
            (session.width(), session.height()),
            (params.sim.worldgen.width, params.sim.worldgen.height),
            "the session serves the generated grid dimensions"
        );
        assert_eq!(session.tick(), 0);
        assert_eq!(session.revision(), 0);
        assert!(session.log().is_empty(), "no commands recorded yet");
        assert_eq!(
            session.heights().len(),
            session.width() as usize * session.height() as usize,
            "the row-major snapshot is w × h",
        );
    }

    #[test]
    fn submit_applies_the_command_and_records_it() {
        let params = params();
        let mut session = WorkbenchSession::new(&params);
        let (sx, sy) = flat_sea_vertex(&session, &params);
        let before = session.world().field().get(sx, sy).unwrap();

        session.submit(TerrainCommand::Raise { x: sx, y: sy });

        assert_eq!(
            session.world().field().get(sx, sy),
            Some(before + 1),
            "a raise on flat sea floor lifts the target one step"
        );
        assert_eq!(session.tick(), 1, "one command advances the logical tick");
        assert_eq!(session.revision(), 1, "a real change bumps the revision");
        assert_eq!(
            session.log(),
            &[(0, TerrainCommand::Raise { x: sx, y: sy })],
            "the command is recorded at the tick it applied on",
        );
    }

    #[test]
    fn a_no_op_submit_records_and_ticks_but_does_not_bump_the_revision() {
        // An out-of-bounds command moves nothing: it is still a submission (it
        // is recorded and advances the tick), but the heights do not change, so
        // the render revision must not bump (ADR 0022 §3).
        let params = params();
        let mut session = WorkbenchSession::new(&params);
        let off_grid = TerrainCommand::Raise {
            x: session.width(),
            y: 0,
        };

        session.submit(off_grid);

        assert_eq!(session.tick(), 1, "a no-op still advances the tick");
        assert_eq!(session.revision(), 0, "an unchanged field does not bump");
        assert_eq!(session.log(), &[(0, off_grid)], "the no-op is still logged");
    }

    #[test]
    fn a_recorded_session_replays_to_a_bit_identical_field() {
        // Record → replay identity (I3, issue #10): a scripted sculpt over the
        // generated world, then the *same log* re-applied to a fresh World from
        // the same params, must yield a bit-identical field. Raises on a flat
        // sea patch grow a stepped cone; a lower reverses one step.
        let params = params();
        let mut session = WorkbenchSession::new(&params);
        let (sx, sy) = flat_sea_vertex(&session, &params);
        let script = [
            TerrainCommand::Raise { x: sx, y: sy },
            TerrainCommand::Raise { x: sx, y: sy },
            TerrainCommand::Raise { x: sx, y: sy },
            TerrainCommand::Lower { x: sx, y: sy },
            TerrainCommand::Raise { x: sx + 1, y: sy },
        ];

        for &command in &script {
            session.submit(command);
        }
        let log = session.log().to_vec();
        assert_eq!(log.len(), script.len(), "every submission was recorded");

        // Reconstruct a fresh world from the same params and re-apply the log.
        let field = generate(&params.sim.worldgen, params.sim.terrain.max_step);
        let features = place_features(&field, &params.sim.worldgen, &params.content.terrain);
        let mut replayed = World::new(field, Some(features));
        for (_, command) in &log {
            replayed.apply(&params.sim.terrain, *command);
        }

        assert_eq!(
            session.world().field(),
            replayed.field(),
            "replaying the recorded log reproduces the field bit-for-bit (I3)",
        );
    }
}
