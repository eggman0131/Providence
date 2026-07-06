//! Validated, immutable simulation parameters — the plain data types the
//! deterministic core consumes (docs/20-architecture.md §2.1, ADR 0008).
//!
//! `no_std`: these types cross into the core, which cannot touch `std`
//! (ADR 0009). The std-side authoring/validation structs (`serde`/`garde`/
//! `schemars`) live in the `config-loader` adapter and map into these types
//! (ADR 0008 as refined by ADR 0009). Field docs name the config key each
//! field carries (docs/40-parameterisation.md §2).

#![no_std]
#![forbid(unsafe_code)]

/// Root of all parameters injected into the deterministic core.
///
/// Constructed only by the `config-loader` adapter after full validation;
/// the core treats it as immutable data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// `sim.*` — deterministic-simulation parameters.
    pub sim: SimParams,
}

/// `sim.*` — parameters governing the deterministic core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimParams {
    /// `sim.placeholder.*` — Phase-1 gate-scaffolding parameters.
    pub placeholder: PlaceholderParams,
}

/// `sim.placeholder.*` — placeholder parameters proving the config → core
/// wiring end-to-end (contract §7.2). Deleted when the first real `sim.*`
/// parameters land in Phase 2 (prefer deletion, contract §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderParams {
    /// `sim.placeholder.tick_increment` — ticks the placeholder state
    /// advances per step.
    pub tick_increment: u64,
}
