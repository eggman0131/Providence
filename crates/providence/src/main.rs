//! Composition root (docs/20-architecture.md §2.3): wires adapters to ports
//! at startup and launches the application.
//!
//! Subcommands (dev tooling — the real game loop lands in later phases):
//! - *(none)* — a smoke run proving the config → params → session pipeline,
//!   plus a textual terrain demo (issue #6 §5) and the interactive command seam
//!   (ADR 0022): a [`WorkbenchSession`] sculpts the generated world through the
//!   [`SimDriver`] port and prints a before/after census.
//! - `workbench` — open the on-screen 3D terrain workbench (issue #8, ADR 0020;
//!   ADR 0022): a lit height field the Director can orbit / pan / zoom **and
//!   shape** — left-click raises the picked vertex, right-click lowers it, and a
//!   drag still moves the camera. Needs a display.
//! - `capture [PATH [YAW PITCH DISTANCE]]` — render the same scene headlessly to
//!   a PNG (ADR 0020 §2), the agents-only visual self-check used by `/verify`.
//!   The optional orbit (yaw/pitch degrees, distance) drives the Phase-2 camera
//!   for the multi-angle self-check; omitted, it uses the configured pose. No
//!   display required.
//! - `capture-shape [BEFORE AFTER]` — the display-free proof of the interactive
//!   shaping seam (ADR 0022): submit a scripted `TerrainCommand` through the
//!   same `SimDriver` submit + snapshot-pull path the event loop uses and
//!   capture before/after PNGs, so the land is *observed* to change without a
//!   display. No display required.
//!
//! The composition root is the only crate permitted to name concrete adapters
//! (docs/20-architecture.md §5.2): it projects `render.*` into `RenderParams`,
//! builds a [`TerrainFrame`] snapshot from a core height field, and hands it to
//! a [`RendererPort`]. The renderer only ever sees the derived snapshot, never
//! the core (ADR 0020 §1).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use providence_app::WorkbenchSession;
use providence_config::{InputParams, Params, RenderParams, TerrainParams};
use providence_core::terrain::{
    Feature, FeatureMap, HeightField, TerrainType, classify_vertex, generate, place_features, raise,
};
use providence_ports::{RendererPort, SimDriver, TerrainCommand, TerrainFrame};
use providence_renderer::{HeadlessRenderer, NoopRenderer, OrbitController, WindowRenderer};

/// Fixed demo values for the smoke run — not behavioural config (the smoke
/// run is dev tooling, not gameplay; real sessions take seed and length
/// from scenario config in later phases).
const SMOKE_SEED: u64 = 0xD1CE;
const SMOKE_STEPS: u64 = 100;

/// Terrain demo dimensions and shaping (dev tooling, not gameplay): an odd
/// side so the field has a true centre vertex, raised a few times to build a
/// visible stepped cone.
const DEMO_SIZE: u32 = 11;
const DEMO_RAISES: u32 = 3;
/// Glyph per integer height for the ASCII heightmap, `0` first. Heights in the
/// demo stay within its length; taller vertices reuse the last glyph.
const HEIGHT_GLYPHS: &[u8] = b".:-=+*#%@";

/// Default output path for a `capture` with no explicit path argument.
const DEFAULT_CAPTURE_PATH: &str = "target/workbench.png";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => smoke_run(),
        Some("workbench") => run_workbench(),
        Some("capture") => run_capture(&args[1..]),
        Some("capture-shape") => run_capture_shape(&args[1..]),
        Some(other) => {
            eprintln!(
                "providence: unknown subcommand `{other}` (try: \
                 workbench | capture [PATH [YAW PITCH DISTANCE]] | capture-shape [BEFORE AFTER])"
            );
            ExitCode::FAILURE
        }
    }
}

/// A parsed `capture` invocation: where to write the PNG and, optionally, an
/// explicit orbit pose (yaw/pitch degrees, distance) for the Phase-2
/// multi-angle self-check.
struct CaptureArgs {
    path: PathBuf,
    pose: Option<(f32, f32, f32)>,
}

/// Parse the `capture` arguments: `[PATH [YAW PITCH DISTANCE]]`. A lone path
/// keeps the configured pose; all four give an explicit orbit. Any other arity
/// (e.g. two args) is a usage error rather than a silent misread.
fn parse_capture_args(args: &[String]) -> Result<CaptureArgs, String> {
    let parse = |raw: &str, name: &str| {
        raw.parse::<f32>()
            .map_err(|_| format!("`{name}` must be a number, got `{raw}`"))
    };
    match args {
        [] => Ok(CaptureArgs {
            path: PathBuf::from(DEFAULT_CAPTURE_PATH),
            pose: None,
        }),
        [path] => Ok(CaptureArgs {
            path: PathBuf::from(path),
            pose: None,
        }),
        [path, yaw, pitch, distance] => Ok(CaptureArgs {
            path: PathBuf::from(path),
            pose: Some((
                parse(yaw, "YAW")?,
                parse(pitch, "PITCH")?,
                parse(distance, "DISTANCE")?,
            )),
        }),
        _ => Err("usage: capture [PATH [YAW PITCH DISTANCE]]".into()),
    }
}

/// The default smoke run: load config, print the textual terrain demo, prove
/// the `RendererPort` seam with the no-op renderer, then advance a session.
fn smoke_run() -> ExitCode {
    let params = match load_params() {
        Ok(params) => params,
        Err(code) => return code,
    };

    let field = print_terrain_demo(&params.sim.terrain);

    let render = match load_render() {
        Ok(render) => render,
        Err(code) => return code,
    };
    present_demo_frame(&field, &render);

    run_shaping_smoke(&params);

    let mut session = providence_app::Session::new(params, SMOKE_SEED);
    for _ in 0..SMOKE_STEPS {
        session.advance();
    }

    println!(
        "providence: gate scaffold OK — tick {} after {} steps (seed {SMOKE_SEED:#x})",
        session.state().tick,
        SMOKE_STEPS
    );
    ExitCode::SUCCESS
}

/// Prove the interactive command seam end-to-end (ADR 0022): build a
/// [`WorkbenchSession`] over the generated world, submit a short scripted sculpt
/// through the [`SimDriver`] port, and print a before/after terrain census — so
/// config → worldgen → session → apply → snapshot is *observed* working, not
/// merely asserted (contract §3). The sculpt raises a flat sea patch (never
/// refused — water carries no immovables), turning open water into a small
/// stepped island, then reverses one step.
fn run_shaping_smoke(params: &Params) {
    let mut session = WorkbenchSession::new(params);
    let (width, height) = (session.width(), session.height());
    println!(
        "providence: interactive seam (ADR 0022) — shaping a {width}×{height} generated world:"
    );
    println!(
        "  before: {}",
        census_line(session.heights(), width, height, params)
    );

    let Some((sx, sy)) = flat_sea_patch(
        session.heights(),
        width,
        height,
        params.sim.worldgen.sea_level,
    ) else {
        println!("  (no open-sea patch to sculpt — skipping the shaping demo)");
        return;
    };

    // Three raises grow a stepped cone out of the sea; one lower reverses a step.
    // Each is a discrete, recorded TerrainCommand through the SimDriver port.
    let sculpt = [
        TerrainCommand::Raise { x: sx, y: sy },
        TerrainCommand::Raise { x: sx, y: sy },
        TerrainCommand::Raise { x: sx, y: sy },
        TerrainCommand::Lower { x: sx, y: sy },
    ];
    for command in sculpt {
        session.submit(command);
    }

    println!(
        "  after:  {}",
        census_line(session.heights(), width, height, params)
    );
    println!(
        "  sculpted vertex ({sx}, {sy}); submitted {n} commands \
         → tick {tick}, revision {revision}, {logged} logged \
         (a session is seed + params + log, replayable bit-for-bit)",
        n = sculpt.len(),
        tick = session.tick(),
        revision = session.revision(),
        logged = session.log().len(),
    );
}

/// A one-line terrain-type census of a row-major height snapshot: how the
/// heights classify into water / shore / land / mountain (ADR 0017 §1) plus the
/// height range and the step invariant — the honest textual observation for the
/// shaping smoke run.
fn census_line(heights: &[i32], width: u32, height: u32, params: &Params) -> String {
    let worldgen = &params.sim.worldgen;
    let terrain = &params.content.terrain;
    let (mut water, mut shore, mut land, mut mountain) = (0_u32, 0_u32, 0_u32, 0_u32);
    let (mut lowest, mut highest) = (i32::MAX, i32::MIN);
    for &h in heights {
        lowest = lowest.min(h);
        highest = highest.max(h);
        match classify_vertex(
            h,
            worldgen.sea_level,
            terrain.shore.band,
            terrain.mountain.min_height,
        ) {
            TerrainType::Water => water += 1,
            TerrainType::Shore => shore += 1,
            TerrainType::Land => land += 1,
            TerrainType::Mountain => mountain += 1,
        }
    }
    let dry = shore + land + mountain;
    let total = width * height;
    format!(
        "water {water}, shore {shore}, land {land}, mountain {mountain} \
         ({dry}/{total} dry); heights {lowest}..={highest}"
    )
}

/// The first interior vertex whose height and all four orthogonal neighbours sit
/// exactly at `sea_level` — a flat patch of sea floor to sculpt (worldgen pins
/// water flat at the datum, ADR 0021). Row-major over the snapshot; `None` if
/// the world has no such patch.
fn flat_sea_patch(heights: &[i32], width: u32, height: u32, sea_level: i32) -> Option<(u32, u32)> {
    let at = |x: u32, y: u32| heights.get((y * width + x) as usize).copied();
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let flat = [(x, y), (x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .all(|&(nx, ny)| at(nx, ny) == Some(sea_level));
            if flat {
                return Some((x, y));
            }
        }
    }
    None
}

/// Open the on-screen 3D workbench (issue #8 Phase 1; interactive shaping,
/// ADR 0022). Builds the interactive [`WorkbenchSession`] (the `SimDriver` the
/// renderer shapes through), seeds the initial frame from its snapshot, and runs
/// the event loop — clicks submit commands, drags move the camera — until the
/// window closes.
fn run_workbench() -> ExitCode {
    let params = match load_params() {
        Ok(params) => params,
        Err(code) => return code,
    };
    let render = match load_render() {
        Ok(render) => render,
        Err(code) => return code,
    };
    let input = match load_input() {
        Ok(input) => input,
        Err(code) => return code,
    };

    // Census from a freshly generated world: the immovables census needs the
    // FeatureMap, which the session does not expose. The session regenerates the
    // identical field internally (worldgen is a pure function of the seed).
    let field = generate_world(&params);
    let features = place_features(&field, &params.sim.worldgen, &params.content.terrain);
    print_terrain_census(&field, &features, &params);

    // The interactive session is the SimDriver the renderer submits commands to
    // and pulls fresh snapshots from (ADR 0022 §4).
    let mut session = WorkbenchSession::new(&params);

    let mut renderer = WindowRenderer::new(render);
    // Seed the initial frame from the session snapshot; `present` is unchanged
    // (ADR 0022 §4). The borrow ends before the session is handed to `run`.
    {
        let frame = TerrainFrame::new(session.width(), session.height(), session.heights());
        renderer.present(frame);
    }
    println!(
        "providence: opening the interactive terrain workbench — click to shape \
         (left raises, right lowers, by default), drag to orbit/pan/zoom. \
         Close the window to exit."
    );
    match renderer.run(&mut session, input) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("providence: workbench error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Render the workbench scene headlessly to a PNG (ADR 0020 §2) — the
/// display-free visual self-check for `/verify`. An optional explicit orbit
/// pose drives the Phase-2 camera so several angles can be captured and compared
/// without a display.
fn run_capture(args: &[String]) -> ExitCode {
    let capture = match parse_capture_args(args) {
        Ok(capture) => capture,
        Err(message) => {
            eprintln!("providence: {message}");
            return ExitCode::FAILURE;
        }
    };
    let (params, render) = match (load_params(), load_render()) {
        (Ok(params), Ok(render)) => (params, render),
        (Err(code), _) | (_, Err(code)) => return code,
    };

    let field = generate_world(&params);
    let features = place_features(&field, &params.sim.worldgen, &params.content.terrain);
    print_terrain_census(&field, &features, &params);
    let heights = frame_heights(&field);
    let frame = TerrainFrame::new(field.width(), field.height(), &heights);

    let mut renderer = HeadlessRenderer::new(render.clone());
    // Adapter-local camera override for the multi-angle self-check (ADR 0020
    // §3): resolve the requested orbit through the same controller the window
    // uses, so a captured angle matches what the Director would see live.
    if let Some((yaw, pitch, distance)) = capture.pose {
        let mut controller = OrbitController::from_params(&render.camera);
        controller.set_pose(yaw, pitch, distance);
        renderer.set_view(controller.camera());
    }
    renderer.present(frame);
    match renderer.capture(&capture.path) {
        Ok(()) => {
            let pose = capture.pose.map_or_else(
                || " (configured pose)".to_string(),
                |(yaw, pitch, distance)| {
                    format!(" (yaw {yaw}°, pitch {pitch}°, distance {distance})")
                },
            );
            println!(
                "providence: captured a {}×{} terrain workbench frame to {}{pose}",
                field.width(),
                field.height(),
                capture.path.display(),
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("providence: capture error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Default before/after PNG paths for a `capture-shape` with no explicit paths.
const DEFAULT_SHAPE_BEFORE: &str = "target/workbench-before.png";
const DEFAULT_SHAPE_AFTER: &str = "target/workbench-after.png";
/// How many raises the headless shaping proof applies — enough to grow a clearly
/// visible stepped cone out of the sea (dev tooling, not gameplay).
const SHAPE_PROOF_RAISES: u32 = 3;

/// The display-free proof of the interactive pick→command→redraw path
/// (ADR 0022; the Definition-of-Done observation, contract §3). Builds a
/// [`WorkbenchSession`], captures a before PNG, submits a scripted sculpt through
/// the **same** [`SimDriver`] submit + snapshot-pull path the window event loop
/// uses, then captures an after PNG — so the land is *observed* to change
/// without a display. The cursor→vertex pick half of the path is unit-tested in
/// the renderer gate (`pick`/`input`); this proves the command→submit→redraw
/// half end-to-end.
fn run_capture_shape(args: &[String]) -> ExitCode {
    let (before_path, after_path) = match args {
        [] => (
            PathBuf::from(DEFAULT_SHAPE_BEFORE),
            PathBuf::from(DEFAULT_SHAPE_AFTER),
        ),
        [before, after] => (PathBuf::from(before), PathBuf::from(after)),
        _ => {
            eprintln!("providence: usage: capture-shape [BEFORE_PNG AFTER_PNG]");
            return ExitCode::FAILURE;
        }
    };
    let params = match load_params() {
        Ok(params) => params,
        Err(code) => return code,
    };
    let render = match load_render() {
        Ok(render) => render,
        Err(code) => return code,
    };

    // The interactive session — the SimDriver the window would shape through.
    let mut session = WorkbenchSession::new(&params);
    let (width, height) = (session.width(), session.height());
    println!(
        "providence: headless shaping proof (ADR 0022) on a {width}×{height} generated world:"
    );
    println!(
        "  before: {}",
        census_line(session.heights(), width, height, &params)
    );
    if let Err(code) = capture_snapshot(&session, &render, &before_path) {
        return code;
    }

    // Find a guaranteed-shapeable vertex — a flat sea patch carries no immovables
    // (ADR 0017 §5), so a raise there always moves the land, never refused.
    let Some((sx, sy)) = flat_sea_patch(
        session.heights(),
        width,
        height,
        params.sim.worldgen.sea_level,
    ) else {
        eprintln!("providence: no open-sea patch to sculpt — cannot run the shaping proof");
        return ExitCode::FAILURE;
    };

    // Submit the scripted sculpt through the SimDriver — the exact submit +
    // snapshot-pull path the event loop's shaping click drives (ADR 0022 §3).
    let before_revision = session.revision();
    for _ in 0..SHAPE_PROOF_RAISES {
        session.submit(TerrainCommand::Raise { x: sx, y: sy });
    }
    println!(
        "  after:  {}",
        census_line(session.heights(), width, height, &params)
    );
    if let Err(code) = capture_snapshot(&session, &render, &after_path) {
        return code;
    }

    println!(
        "  sculpted vertex ({sx}, {sy}); {n} raises → revision {before_revision}→{after_revision}, \
         {logged} commands logged; before {before}, after {after} \
         (the land changed — a session is seed + params + log, replayable bit-for-bit)",
        n = SHAPE_PROOF_RAISES,
        after_revision = session.revision(),
        logged = session.log().len(),
        before = before_path.display(),
        after = after_path.display(),
    );
    ExitCode::SUCCESS
}

/// Capture the session's current snapshot to a PNG through the headless renderer
/// — the same `present` → build-mesh path the window uses each redraw (ADR 0022).
fn capture_snapshot(
    session: &WorkbenchSession,
    render: &RenderParams,
    path: &Path,
) -> Result<(), ExitCode> {
    let frame = TerrainFrame::new(session.width(), session.height(), session.heights());
    let mut renderer = HeadlessRenderer::new(render.clone());
    renderer.present(frame);
    renderer.capture(path).map_err(|error| {
        eprintln!("providence: capture error: {error}");
        ExitCode::FAILURE
    })
}

/// Load and validate the core params from `config/`, mapping a failure to a
/// printed error and a failure exit code.
fn load_params() -> Result<Params, ExitCode> {
    providence_config_loader::load_dir(Path::new("config")).map_err(|error| {
        eprintln!("providence: config error: {error}");
        ExitCode::FAILURE
    })
}

/// Load and validate the presentation params (`render.*`) from `config/`.
fn load_render() -> Result<RenderParams, ExitCode> {
    providence_config_loader::load_render(Path::new("config")).map_err(|error| {
        eprintln!("providence: render config error: {error}");
        ExitCode::FAILURE
    })
}

/// Load and validate the input params (`input.*`) from `config/` (ADR 0022) —
/// the interactive workbench's shaping bindings.
fn load_input() -> Result<InputParams, ExitCode> {
    providence_config_loader::load_input(Path::new("config")).map_err(|error| {
        eprintln!("providence: input config error: {error}");
        ExitCode::FAILURE
    })
}

/// Generate the workbench world from the seeded worldgen config (ADR 0021):
/// the real terrain #11 judges, replacing the hand-built demo bump. The field
/// already satisfies the step invariant; `max_step` is the invariant it must
/// honour.
fn generate_world(params: &Params) -> HeightField {
    generate(&params.sim.worldgen, params.sim.terrain.max_step)
}

/// Print a terrain-type census of a generated world — the honest, textual
/// "verified" observation (contract §3): how the seed's heights classify into
/// water / shore / land / mountain (ADR 0017 §1), reading the `content.terrain.*`
/// thresholds. Proves worldgen + the derivations are wired end-to-end before the
/// 3D view even opens.
fn print_terrain_census(field: &HeightField, features: &FeatureMap, params: &Params) {
    let worldgen = &params.sim.worldgen;
    let terrain = &params.content.terrain;
    let (mut water, mut shore, mut land, mut mountain) = (0_u32, 0_u32, 0_u32, 0_u32);
    let (mut trees, mut rocks) = (0_u32, 0_u32);
    let (mut lowest, mut highest) = (i32::MAX, i32::MIN);
    for y in 0..field.height() {
        for x in 0..field.width() {
            let height = field.get(x, y).unwrap_or(worldgen.sea_level);
            lowest = lowest.min(height);
            highest = highest.max(height);
            match classify_vertex(
                height,
                worldgen.sea_level,
                terrain.shore.band,
                terrain.mountain.min_height,
            ) {
                TerrainType::Water => water += 1,
                TerrainType::Shore => shore += 1,
                TerrainType::Land => land += 1,
                TerrainType::Mountain => mountain += 1,
            }
            match features.get(x, y) {
                Some(Feature::Tree) => trees += 1,
                Some(Feature::Rock) => rocks += 1,
                None => {}
            }
        }
    }
    let total = field.width() * field.height();
    let dry = shore + land + mountain;
    println!(
        "providence: generated a {w}×{h} {shape:?} world (seed {seed}) — \
         {dry}/{total} vertices dry ({percent}%): \
         water {water}, shore {shore}, land {land}, mountain {mountain}; \
         immovables: {trees} trees, {rocks} rock; \
         heights {lowest}..={highest}, invariant held = {ok}",
        w = field.width(),
        h = field.height(),
        shape = worldgen.shape,
        seed = worldgen.seed,
        percent = dry * 100 / total.max(1),
        ok = field.satisfies_step_invariant(params.sim.terrain.max_step),
    );
}

/// Flatten a height field into the row-major buffer a [`TerrainFrame`] borrows.
fn frame_heights(field: &HeightField) -> Vec<i32> {
    let mut heights = Vec::with_capacity(field.width() as usize * field.height() as usize);
    for y in 0..field.height() {
        for x in 0..field.width() {
            heights.push(field.get(x, y).unwrap_or_default());
        }
    }
    heights
}

/// Build a flat field, raise its centre `DEMO_RAISES` times, and print the
/// resulting stepped plateau as an ASCII heightmap — the honest, textual
/// "verified" observation for issue #6 before the 3D workbench (§5).
/// Returns the built field so the workbench seam (below) can present it.
fn print_terrain_demo(terrain: &TerrainParams) -> HeightField {
    let mid = DEMO_SIZE / 2;
    let mut field = HeightField::flat(DEMO_SIZE, DEMO_SIZE, 0);

    let mut total_moved: u32 = 0;
    let mut total_cost: u64 = 0;
    for _ in 0..DEMO_RAISES {
        // The shaping demo carries no immovables (None); the workbench world
        // does (see run_workbench / print_terrain_census).
        let outcome = raise(&mut field, mid, mid, terrain, None);
        total_moved += outcome.moved;
        total_cost += outcome.cost;
    }

    println!(
        "providence: terrain demo — {size}×{size} field, centre raised {n}× \
         (max_step {step}, ceiling {ceiling}):",
        size = DEMO_SIZE,
        n = DEMO_RAISES,
        step = terrain.max_step,
        ceiling = terrain.max_height,
    );
    for y in 0..field.height() {
        let mut row = String::new();
        for x in 0..field.width() {
            let height = field.get(x, y).unwrap_or_default();
            row.push(glyph_for(height));
        }
        println!("  {row}");
    }
    println!(
        "  moved {total_moved} vertices, cost {total_cost}, invariant held = {}",
        field.satisfies_step_invariant(terrain.max_step),
    );

    field
}

/// Present the demo field through the no-op renderer — the GPU-free proof that
/// the `RendererPort` seam (ADR 0020) is wired end-to-end. Builds a row-major
/// [`TerrainFrame`] snapshot from the field and hands it to a [`NoopRenderer`],
/// exactly as the on-screen adapter does. `render` is echoed to show the
/// presentation-config projection is live.
fn present_demo_frame(field: &HeightField, render: &RenderParams) {
    let heights = frame_heights(field);
    let frame = TerrainFrame::new(field.width(), field.height(), &heights);
    let mut renderer = NoopRenderer::new();
    renderer.present(frame);

    println!(
        "providence: workbench seam OK — presented a {w}×{d} frame via NoopRenderer \
         ({n} frame(s); palette low {low:?}, background {bg:?})",
        w = field.width(),
        d = field.height(),
        n = renderer.presented(),
        low = render.palette.low_rgb,
        bg = render.background.rgb,
    );
}

/// Map an integer height to its ASCII glyph, saturating at the tallest glyph.
fn glyph_for(height: i32) -> char {
    let index = usize::try_from(height.max(0)).unwrap_or(0);
    let last = HEIGHT_GLYPHS.len() - 1;
    char::from(HEIGHT_GLYPHS[index.min(last)])
}
