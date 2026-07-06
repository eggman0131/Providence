//! Authoring structs — the single source of truth for the config schema
//! (ADR 0008): `serde` shapes the keys (`deny_unknown_fields` rejects
//! anything outside them), `garde` carries ranges and cross-key invariants,
//! `schemars` generates the committed JSON Schema.
//!
//! These are std types; they map into the `no_std` `providence-config`
//! param types via [`ConfigRoot::into_params`] (ADR 0009 refinement).

use garde::Validate;
use providence_config::{Params, PlaceholderParams, SimParams};
use schemars::JsonSchema;
use serde::Deserialize;

/// Root of the authored configuration (all layers merged).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConfigRoot {
    /// `meta.*` — config/schema versioning and provenance.
    #[garde(dive)]
    pub meta: MetaSection,
    /// `sim.*` — deterministic-simulation parameters.
    #[garde(dive)]
    pub sim: SimSection,
}

/// `meta.*` (docs/40-parameterisation.md §2.2).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MetaSection {
    /// `meta.schema_version` — the schema this config targets; a mismatch
    /// triggers the migration path, never a silent misread.
    #[garde(range(min = 1))]
    pub schema_version: u32,
}

/// `sim.*` (docs/40-parameterisation.md §2.2).
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct SimSection {
    /// `sim.placeholder.*` — Phase-1 gate scaffolding (contract §7.2);
    /// deleted when real `sim.*` parameters land in Phase 2.
    #[garde(dive)]
    pub placeholder: PlaceholderSection,
}

/// `sim.placeholder.*` — placeholder parameters proving config → core wiring.
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderSection {
    /// `sim.placeholder.tick_increment` — ticks the placeholder state
    /// advances per step. Hot-reloadable (a pure balance value).
    #[garde(range(min = 1))]
    pub tick_increment: u64,
}

impl ConfigRoot {
    /// Map the validated authoring config into the immutable `no_std`
    /// params the core consumes. Purely mechanical; covered by tests.
    #[must_use]
    pub fn into_params(self) -> Params {
        Params {
            sim: SimParams {
                placeholder: PlaceholderParams {
                    tick_increment: self.sim.placeholder.tick_increment,
                },
            },
        }
    }
}
