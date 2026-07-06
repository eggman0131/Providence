# contracts/ — machine-readable schemas

This directory is the home for **machine-readable contract artifacts** — primarily the **configuration schema** that enforces [`../40-parameterisation.md`](../40-parameterisation.md) (allowed keys, types, ranges, registered namespace roots), and any other machine-checkable interface contracts (e.g. the LLM `Observation` / `StrategyDecision` schemas from [`../50-llm-opponent.md`](../50-llm-opponent.md)).

## Contents

- [`config.schema.json`](./config.schema.json) — the machine-readable schema for all configuration keys. **Generated, do not edit by hand:** the source of truth is the authoring structs in `adapters/config-loader` (types-first, [ADR 0008](../decisions/0008-toml-config-format-types-first-schema.md)). Regenerate with `cargo xtask schema --write`; the gate's regenerate-and-diff check fails on any drift. Ranges flow into the schema automatically from the `garde` validation attributes.

## What lands here later

- The `Observation` and `StrategyDecision` schemas for the LLM port (Phase 5).
- Any versioned migration definitions referenced by `meta.schema_version`.

Adding or changing a schema follows the contract's change rules; introducing a new namespace root requires an ADR.
