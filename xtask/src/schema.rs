//! Schema regenerate-and-diff (ADR 0008): the committed JSON Schema artifact
//! must exactly match what the authoring structs generate — types and schema
//! can never drift.

use std::fs;
use std::process::ExitCode;

/// The committed machine-readable schema (docs/40-parameterisation.md §4;
/// lives in the reserved `docs/contracts/` home).
pub const SCHEMA_ARTIFACT: &str = "docs/contracts/config.schema.json";

/// Fail if the committed artifact differs from the generated schema.
pub fn check() -> Result<(), String> {
    let path = crate::workspace_root().join(SCHEMA_ARTIFACT);
    let committed = fs::read_to_string(&path).map_err(|error| {
        format!(
            "cannot read {SCHEMA_ARTIFACT}: {error}; run `cargo xtask schema --write` \
             to generate it"
        )
    })?;
    let generated = providence_config_loader::schema_json();
    if committed == generated {
        Ok(())
    } else {
        Err(format!(
            "{SCHEMA_ARTIFACT} is out of date with the authoring structs; \
             run `cargo xtask schema --write` and commit the result"
        ))
    }
}

/// Regenerate the committed artifact from the authoring structs.
pub fn write() -> ExitCode {
    let path = crate::workspace_root().join(SCHEMA_ARTIFACT);
    match fs::write(&path, providence_config_loader::schema_json()) {
        Ok(()) => {
            println!("wrote {SCHEMA_ARTIFACT}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {SCHEMA_ARTIFACT}: {error}");
            ExitCode::FAILURE
        }
    }
}
