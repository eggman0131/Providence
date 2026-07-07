//! Composition root (docs/20-architecture.md §2.3): wires adapters to ports
//! at startup and launches the application.
//!
//! Phase-1 scope: a smoke binary proving the config → params → session
//! pipeline end-to-end (contract §3 "Verified"). It also renders a small
//! terrain shaping demo (issue #6 §5) — the pre-workbench, textual surrogate
//! for "seen and felt": a stepped plateau you can eyeball until the 3D
//! workbench (#8/#9) lets the land be seen in motion.
//!
//! Issue #8 Pre-work (ADR 0020) additionally wires the `RendererPort` seam,
//! GPU-free: it projects the presentation config into `RenderParams`, builds a
//! [`TerrainFrame`] snapshot from the demo field, and presents it through the
//! no-op renderer — proving the port, snapshot, adapter, and render-config
//! projection compile and run before any real renderer exists. The on-screen
//! `wgpu`/`winit` adapter and its `workbench` subcommand land in Phase 1.

use std::path::Path;
use std::process::ExitCode;

use providence_config::{RenderParams, TerrainParams};
use providence_core::terrain::{HeightField, raise};
use providence_ports::{RendererPort, TerrainFrame};
use providence_renderer::NoopRenderer;

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

fn main() -> ExitCode {
    let params = match providence_config_loader::load_dir(Path::new("config")) {
        Ok(params) => params,
        Err(error) => {
            eprintln!("providence: config error: {error}");
            return ExitCode::FAILURE;
        }
    };

    let field = print_terrain_demo(&params.sim.terrain);

    // Issue #8 Pre-work (ADR 0020): prove the RendererPort seam end-to-end
    // before any GPU code — load the presentation config and present the demo
    // field through the no-op renderer.
    let render = match providence_config_loader::load_render(Path::new("config")) {
        Ok(render) => render,
        Err(error) => {
            eprintln!("providence: render config error: {error}");
            return ExitCode::FAILURE;
        }
    };
    present_demo_frame(&field, &render);

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

/// Build a flat field, raise its centre `DEMO_RAISES` times, and print the
/// resulting stepped plateau as an ASCII heightmap — the honest, textual
/// "verified" observation for issue #6 before the 3D workbench exists (§5).
/// Returns the built field so the workbench seam (below) can present it.
fn print_terrain_demo(terrain: &TerrainParams) -> HeightField {
    let mid = DEMO_SIZE / 2;
    let mut field = HeightField::flat(DEMO_SIZE, DEMO_SIZE, 0);

    let mut total_moved: u32 = 0;
    let mut total_cost: u64 = 0;
    for _ in 0..DEMO_RAISES {
        let outcome = raise(&mut field, mid, mid, terrain);
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
/// the `RendererPort` seam (ADR 0020) is wired end-to-end before any real
/// renderer exists. Builds a row-major [`TerrainFrame`] snapshot from the field
/// and hands it to a [`NoopRenderer`], exactly as the on-screen adapter will.
/// `render` is loaded and echoed to show the presentation-config projection is
/// live, though the no-op renderer draws nothing with it.
fn present_demo_frame(field: &HeightField, render: &RenderParams) {
    let mut heights = Vec::with_capacity(field.width() as usize * field.height() as usize);
    for y in 0..field.height() {
        for x in 0..field.width() {
            heights.push(field.get(x, y).unwrap_or_default());
        }
    }

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
