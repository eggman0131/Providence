# 0009 — Enforcement tooling and the one-command gate

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§3, §6, §7, §9; I1–I5, I8, I9), [`../40-parameterisation.md`](../40-parameterisation.md) (§6), [`../20-architecture.md`](../20-architecture.md), [`0004`](./0004-deterministic-core-ports-and-adapters.md), [`0006`](./0006-rust-language-and-runtime.md), [`0008`](./0008-toml-config-format-types-first-schema.md)

## Context

The contract is **enforcement-first** (§7): the tooling that keeps every invariant honest must exist and be **green on an empty project before any domain code** — there is no human reviewer, so "anything not enforced by a tool is only a recommendation" (§6.1). This ADR selects the concrete tools for the capabilities the contract requires (§6.2), the config checks in [`40-parameterisation.md`](../40-parameterisation.md) §6, and the determinism/replay harness (§7.1); defines the single gate command (§9); and records two enforcement-strictness choices made this session: an **airtight `no_std` core** and **standard-plus-curated-pedantic** clippy. Language is Rust ([ADR 0006](./0006-rust-language-and-runtime.md)); config is TOML with a types-first schema ([ADR 0008](./0008-toml-config-format-types-first-schema.md)).

## Decision

### 1. Tool selection

| Capability (§6.2 / §7.1 / 40§6) | Tool |
|---|---|
| Static type check, zero-error | `cargo check` (rustc); warnings denied via workspace `[lints]` |
| Formatter, canonical | `rustfmt` (`cargo fmt --check`), pinned `rustfmt.toml`, **stable options only** |
| Linter | `clippy -D warnings`; **standard + curated pedantic** (below) |
| External-dependency policy (I8) | `cargo-deny` (`deny.toml`: advisories, license allow-list, bans, sources, duplicate versions) |
| Internal boundary direction (I2/I4) | **custom** manifest check (§3 below) |
| No cycles (I2) | free — Cargo forbids cyclic crate deps |
| Test runner + coverage (I5) | `cargo test` + `cargo-llvm-cov` (`--fail-under`) |
| Determinism/replay (I3, §7.1) | **custom** replay harness (§3 below) |
| Config schema validation (I1, §2) | `serde` `deny_unknown_fields` + `garde` + `schemars` regen-diff (ADR 0008) |
| Magic-number scan (I1, 40§6.2) | **custom** `syn` scan (§3 below) |
| Key-reference integrity (40§6.3) | **custom** check (§3 below) |
| One-command gate (I9) | `xtask` binary → **`cargo gate`** alias |
| One-command provision (I9) | `cargo xtask setup` (pinned installs) |

**Clippy level:** all default lints denied, plus a **curated subset of pedantic lints** (high-value ones enabled; noisy ones excluded), configured via workspace `[lints.clippy]` and per-crate `clippy.toml`. The curated set starts small and is tuned over time (a non-ADR change); it is version-controlled so changes are reviewable.

### 2. Core purity — `no_std` (airtight)

- The `core` crate is `#![no_std]` + `alloc`, **zero external dependencies**. I/O, wall-clock, networking, threads, and ambient RNG live in `std` and are therefore **not reachable** from the core — I3 purity is structurally impossible to violate, not merely linted. `#![forbid(unsafe_code)]` remains (ADR 0006).
- **Determinism dividend:** `std::collections::HashMap` is unavailable, so the core uses `alloc`'s `Vec`/`BTreeMap`/`BTreeSet` — the deterministic-ordering collections ADR 0006 already mandated; accidental nondeterministic iteration cannot occur in the core.
- **Floating point:** the core commits to **integer/fixed-point** math (ADR 0006's preference). Where a transcendental is genuinely unavoidable, the only permitted helper is `libm` (tiny, pure, `no_std`, offline), used deterministically. Prefer fixed-point; `libm` is the escape hatch.
- **Belt-and-suspenders:** clippy `disallowed_methods` / `disallowed_types` / `disallowed_macros` in `core` ban any nondeterministic API that `alloc` might still expose.

### 3. Custom checks (built in Phase 1, per I8)

- **Boundary-manifest check.** Parse every crate's `Cargo.toml` and assert internal dependency edges are a subset of the allowed DAG: `core → {}`, `config → {}`, `ports → {}`, `app → {core, config, ports}`, `adapters/* → {ports, config}`; adapters must not depend on each other; only the composition-root binary depends on adapters. Any illegal edge fails the gate. (Cargo already guarantees acyclicity.)
- **Determinism/replay harness.** Run the core over a fixed seed + scripted command sequence; assert two runs yield a **bit-identical state history** via a stable, ordered state hash (enabled by the no-`HashMap` rule), and compare against a committed **golden hash** (record–replay regression). The golden updates only on an intentional, reviewed core change.
- **Magic-number scan.** A `syn`-based scan of the `core` crate flags behavioural numeric/string literals outside a tiny allow-list (`0`, `1`, small structural indices), excluding `#[cfg(test)]`. Legitimate exceptions require an explicit, grep-able annotation — never a silent pass.
- **Config schema checks (ADR 0008).** (a) **regen-and-diff** the `schemars` JSON Schema against the committed artifact (drift fails); (b) **validate** every shipped config layer and the merged whole (deserialize + `garde`); (c) **key-reference integrity** — keys read by code exist in the schema and every schema key is reachable; orphans on either side are flagged.

### 4. The gate and provisioning (I9)

- An `xtask` workspace binary runs every check as pinned Rust code, exposed as **`cargo gate`** via a `.cargo/config.toml` alias. Green = all checks pass and is the single definition of "done" (§3). It runs fully locally; there is no cloud CI.
- `cargo xtask setup` provisions the environment: `rust-toolchain.toml` pins the stable channel/version + `rustfmt`/`clippy` components; the pinned external subcommands `cargo-llvm-cov` and `cargo-deny` are installed via `cargo install --locked --version …`. First run needs network (dev-time only, permitted by [`60-constraints.md`](../60-constraints.md) §2); thereafter cached/offline.
- **Bootstrap (§7.2):** the gate is stood up and proven **green on a placeholder workspace** (a trivial `no_std` core module + a trivial passing test + a trivial config + generated schema) before any domain code exists.
- **Pinning (I8/I9):** `rust-toolchain.toml`, `Cargo.lock`, and exact external-tool versions in `setup` make the environment reproducible; versions are recorded and refreshed deliberately.

### 5. Proposed coverage thresholds

Gate-config knobs (tunable without an ADR): `core` ≥ 90% (highest, per I5), `app`/`adapters` ≥ 70%; the gate fails under threshold. Adjusted as the code matures.

## Consequences

- **Positive:**
  - Every item of the §3 Definition of Done is mechanised; "green" has one meaning and one command.
  - Core purity (I3) is **structural**, not disciplined — the strongest possible guarantee — and the crate graph makes boundaries (I2/I4) a compile-time fact, with the manifest check catching illegal *declared* edges.
  - `no_std` forces deterministic collections, reinforcing the replay harness.
  - Fully local and reproducible (I9); a healthy, pinned, minimal tool set (I8).
- **Negative / trade-offs:**
  - Four custom checks are real Phase-1 engineering (boundary-manifest, replay harness, magic-number scan, key-reference integrity).
  - `no_std` adds ergonomic friction (explicit `alloc` imports; `libm`/fixed-point for floats) and **refines [ADR 0008](./0008-toml-config-format-types-first-schema.md)**: the schema/authoring structs (std, `serde`/`garde`/`schemars`) move to the `config-loader` adapter and **map** into the `no_std` `config` crate's plain param types the core reads. The schema stays single-source (generated from the authoring structs); the core param types are a downstream projection connected by a mechanical, tested mapping.
  - The curated-pedantic clippy list needs occasional maintenance; the magic-number scan needs a good allow-list + escape hatch to avoid false positives; the replay golden-hash needs disciplined stable state serialisation; external tools need a one-time online install.
- **Enforcement / gate impact:** this ADR *is* the gate — it wires §6.2, §7.1, and 40§6 into `cargo gate`, validated green on an empty project (§7.2) before any domain code. It resolves §9's deferred gate-command name and §6.2's deferred tool choices.
- **Docs to update (this change):** `decisions/README.md` (index + open list); `30-ai-agent-contract.md` (§6.2 tools now chosen, §9 gate command named); `20-architecture.md` (core crate is `no_std`); `0008` (refinement note for the config split). No invariant *values* change.

## Alternatives considered

- **`std` core + disallowed-lints + import scan** (instead of `no_std`). Lower friction and fully compatible with ADR 0008's single config-struct source, but purity would rest on the *completeness* of a ban-list rather than being structurally impossible. Rejected in favour of the airtight `no_std` guarantee, accepting the config split.
- **Standard-only or full-pedantic+nursery clippy.** Standard-only misses subtler issues; full pedantic+nursery generates frequent false positives that invite `#[allow]`-sprinkling. The curated middle was chosen.
- **`make` / `just` as the gate runner.** Both add an external tool and move orchestration logic out of pinned Rust. `xtask` needs no new dependency and keeps the custom checks as first-class Rust. Rejected.
- **Off-the-shelf internal-boundary tools.** No Cargo-native tool enforces per-crate dependency *direction* well; `cargo-deny` governs external deps only. A small custom manifest check is more precise and I8-aligned than adopting a heavier tool. Rejected.
