//! Composition root (docs/20-architecture.md §2.3): wires adapters to ports
//! at startup and launches the application.
//!
//! Phase-1 scope: a smoke binary proving the config → params → session
//! pipeline end-to-end (contract §3 "Verified"). Renderer/input/persistence
//! wiring lands in Phase 4+.

use std::path::Path;
use std::process::ExitCode;

/// Fixed demo values for the smoke run — not behavioural config (the smoke
/// run is dev tooling, not gameplay; real sessions take seed and length
/// from scenario config in later phases).
const SMOKE_SEED: u64 = 0xD1CE;
const SMOKE_STEPS: u64 = 100;

fn main() -> ExitCode {
    let params = match providence_config_loader::load_dir(Path::new("config")) {
        Ok(params) => params,
        Err(error) => {
            eprintln!("providence: config error: {error}");
            return ExitCode::FAILURE;
        }
    };

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
