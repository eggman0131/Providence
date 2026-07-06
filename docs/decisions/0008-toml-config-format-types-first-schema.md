# 0008 — TOML config format with a types-first schema

- **Status:** Accepted
- **Refined by:** [0009](./0009-enforcement-tooling-and-the-gate.md) — the `no_std` core means the `serde`/`garde`/`schemars` authoring structs live in the `config-loader` adapter (std) and **map** into the `no_std` `config` crate's plain param types; the schema stays generated from the authoring structs (single source), the core param types are a downstream projection.
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../40-parameterisation.md`](../40-parameterisation.md), [`../20-architecture.md`](../20-architecture.md), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (I1, I3, I4, I8, I9), [`0004`](./0004-deterministic-core-ports-and-adapters.md), [`0006`](./0006-rust-language-and-runtime.md), [`0009`](./0009-enforcement-tooling-and-the-gate.md)

## Context

Parameterisation-first (**I1**) makes configuration the primary surface of this game: every rate, cost, threshold, and the entire content catalogue lives in config, not code. [`40-parameterisation.md`](../40-parameterisation.md) §5 left the concrete file format to a later ADR while fixing the requirements it must meet:

- **Namespacing (§2):** every key is a hierarchical **dot-path** under a registered top-level root (`sim.*`, `content.*`, `ai.*`, `render.*`, `input.*`, `runtime.*`, `meta.*`); keys outside the roots are rejected.
- **Content tables (§3):** catalogue content (powers, terrain types, scenarios) is **keyed records with named fields**, not deep arbitrary trees.
- **Format requirements (§4):** human-editable & **commentable**; **schema-validated** (machine-readable schema as source of truth); **layered overrides** (defaults → content pack → user/local, merged and validated as a whole); **versioned** (`meta.schema_version` + migration); **ranges & cross-key invariants** (`min ≤ max`); **hot-reload marking**.
- **Loading (§5):** the core never reads files; `ConfigPort` loads and validates into an **immutable parameter object** injected inward (I3/I4).

Two forces from earlier decisions apply: the codebase is **agent-authored**, so *model competence* and *dependency health* are first-class selection criteria (see [ADR 0006](./0006-rust-language-and-runtime.md), [ADR 0007](./0007-wgpu-rendering-framework.md)); and the implementation language is **Rust**.

## Decision

Configuration is authored in **TOML**, validated by a **types-first** schema in which Rust config structs are the single source of truth.

**On disk — TOML, layered.**
- Config is a set of **TOML** files composed in a defined order: **built-in defaults → scenario/content pack → user/local overrides** (§4). Later layers override earlier by dotted key; the *merged whole* is what gets validated.
- The §2 dot-path namespacing maps directly onto TOML's native tables/dotted keys (`[sim.economy.mana]` / `sim.economy.mana.regen_rate = …`); §3 keyed-record content maps onto TOML tables and arrays-of-tables (`[content.powers.flood]`, `[[content.scenarios]]`). Comments and multi-line strings (for `ai.llm.*` prompt templates) are used directly.

**Schema — types-first (Rust structs are the source of truth).**
- Config deserialises into typed Rust structs with **`serde`** + `#[serde(deny_unknown_fields)]`. The top-level struct's fields are exactly the registered namespace roots, so a key outside them (or an unknown key anywhere) **fails deserialisation** — this is the mechanism behind §2.2 / §2.4 namespace enforcement.
- Semantic validation — ranges and **cross-key invariants** (§4) — uses **`garde`** (derive-based, actively maintained, supports custom/context-aware checks for `min ≤ max`-style rules).
- **`schemars`** generates a **JSON Schema artifact** from the structs, committed to the repo. It drives editor autocomplete/validation on the TOML files (via Taplo) and is the machine-readable schema §4 requires; the gate **regenerates-and-diffs** it so the types and schema can never drift. Hot-reload marking (§4) is a field-level annotation emitted into the schema as a custom keyword.

**Load pipeline (the `ConfigPort` adapter).**
1. Parse each layer with the `toml` crate into a generic value.
2. Deep-merge layers by dotted key into one effective tree.
3. Read `meta.schema_version`; on mismatch, run the defined **migration** path (never a silent misread).
4. Deserialise the merged whole into the **immutable** typed parameter object (`deny_unknown_fields` catches stray keys here).
5. Run `garde` validation (ranges, cross-key invariants); emit **clear, actionable errors** (file, key, expected vs actual).
6. Inject the immutable params inward. The core consumes plain data and never touches a file (I3/I4).

**Dependencies (I8):** `serde`, `toml`, `schemars`, `garde` — all pure-Rust, serde-ecosystem, offline, actively maintained; pinned at latest stable and recorded. The specific **tool versions and their wiring into the gate** (the schema-validation check) are pinned in the enforcement-tooling ADR; this ADR fixes the format, the schema approach, and the crates.

## Consequences

- **Positive:**
  - The mandated dot-path namespace model is TOML's native syntax → near-zero impedance between §2 and the file format.
  - TOML is Rust-native (Cargo) and low-footgun, so **agents author it competently** and the human director reads/annotates it easily; no YAML-style coercion/whitespace traps.
  - **Single source of truth:** change the struct, the schema regenerates; the gate's regenerate-and-diff makes drift impossible.
  - Unknown-key / out-of-namespace rejection is **structural** (`deny_unknown_fields`), not a bolted-on lint (enforces §2.2).
  - Cross-key invariants (`min ≤ max`) are natural in `garde` — the thing pure JSON Schema handles poorly.
  - Editor validation/autocomplete on TOML via Taplo + the generated schema; healthy, minimal dependency set (I8).
  - Clean `ConfigPort` adapter; the core stays pure and deterministic (I3/I4).
- **Negative / trade-offs:**
  - TOML is verbose for any *deeply* nested inline structure; mitigated because the content model is flat keyed records (§3), but a future deeply-nested need would be awkward.
  - The JSON Schema is *generated*, not hand-tuned — custom needs (e.g. the hot-reload keyword) require `schemars` customisation.
  - Layering/deep-merge and the migration path are real machinery that must be written and tested (not free with the format).
  - Commits us to `garde` for validation; if it proves limiting, swapping to `validator` is a later dependency change (ADR/changelog).
- **Enforcement / gate impact:** adds (a) a **schema regenerate-and-diff** check; (b) **config-validates** — every layer and the merged whole deserialise + `garde`-validate; (c) **key-reference integrity** (§6.3) — keys read by code exist in the schema, orphans flagged; (d) **namespace conformance** (§2.4) via `deny_unknown_fields` + root check. This ADR is what makes §6's content-only-change test and magic-number scan enforceable. Versions pinned.
- **Docs to update (this change):** `decisions/README.md` (index + open list), `40-parameterisation.md` (§5 intro + §4 now name TOML + types-first), `20-architecture.md` (`ConfigPort` row). No invariant changes.

## Alternatives considered

- **YAML.** Best for deep nesting and widely known, but footguns (Norway problem, significant whitespace, type coercion) hurt an agent-authored file, and the canonical `serde_yaml` crate was **deprecated/archived in 2024**, leaving a fragmented ecosystem — against I8. Rejected.
- **JSON5 / JSONC.** JSON + comments + nesting, with native JSON Schema, but a weaker/less-mature Rust crate than `toml`/`serde_json` and a less-native feel. Rejected.
- **RON.** Mirrors Rust types elegantly and supports comments, but **niche** — thin training representation (fails the model-competence criterion) and a small ecosystem. Rejected.
- **Plain JSON.** Rock-solid tooling and native JSON Schema, but **no comments** — disqualifying for human-annotated config (§4). Rejected.
- **Schema-first (hand-authored JSON Schema as source of truth).** Language-agnostic and explicit, but Rust types drift from it without extra machinery, and arithmetic cross-key invariants are awkward in pure JSON Schema. Rejected in favour of generating the schema from the types.
