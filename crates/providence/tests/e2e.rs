//! End-to-end: the SHIPPED default config loads, validates, and drives the
//! deterministic core — exercised, not just asserted (contract §3
//! "Verified"; docs/40-parameterisation.md §6.1).

use std::path::Path;

use providence_config_loader::{Layer, load_dir, params_from_layers};

fn config_dir() -> &'static Path {
    // The workspace root, relative to this crate's manifest.
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config"))
}

#[test]
fn shipped_default_config_loads_and_drives_the_core() {
    let params = load_dir(config_dir()).expect("shipped default.toml must load and validate");
    let increment = params.sim.placeholder.tick_increment;

    let mut a = providence_app::Session::new(params.clone(), 7);
    let mut b = providence_app::Session::new(params, 7);
    for _ in 0..50 {
        a.advance();
        b.advance();
    }
    assert_eq!(
        a.state(),
        b.state(),
        "same config + seed must stay bit-identical (I3)"
    );
    assert_eq!(
        a.state().tick,
        50 * increment,
        "tick must advance by the configured increment"
    );
}

#[test]
fn config_only_change_changes_behaviour_with_no_source_edit() {
    // The no-code-change rule (docs/40-parameterisation.md §6.1), through the
    // real loader: overlay a user layer on the SHIPPED default file and
    // observe different behaviour — zero source edits.
    let default_text = std::fs::read_to_string(config_dir().join("default.toml"))
        .expect("shipped default.toml must be readable");
    let default_layer = Layer {
        name: "default.toml".into(),
        text: default_text,
    };
    let overlay = Layer {
        name: "test-overlay".into(),
        text: "[sim.placeholder]\ntick_increment = 7\n".into(),
    };

    let baseline =
        params_from_layers(std::slice::from_ref(&default_layer)).expect("default layer must load");
    let tuned = params_from_layers(&[default_layer, overlay]).expect("overlay must load");

    let mut baseline_session = providence_app::Session::new(baseline, 7);
    let mut tuned_session = providence_app::Session::new(tuned, 7);
    for _ in 0..3 {
        baseline_session.advance();
        tuned_session.advance();
    }
    assert_eq!(
        tuned_session.state().tick,
        21,
        "tuned increment must be observable"
    );
    assert_ne!(
        baseline_session.state().tick,
        tuned_session.state().tick,
        "a config-only change must change observable behaviour"
    );
}
