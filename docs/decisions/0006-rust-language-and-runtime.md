# 0006 — Rust as the implementation language & runtime

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../60-constraints.md`](../60-constraints.md), [`../20-architecture.md`](../20-architecture.md), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (I2, I3, I4, I8, I9), [`0004`](./0004-deterministic-core-ports-and-adapters.md), [`0005`](./0005-macbook-only-offline-runtime.md)

## Context

The programming language and runtime were left open (see the decisions index) and must be fixed before the enforcement framework (contract §7) can be built, since every gate tool is language-specific. The choice is tightly constrained by decisions already accepted:

- **I3 — bit-for-bit determinism** is a hard pillar: the core must reproduce identical state histories across agent sessions, verified by a replay harness. This rewards a language giving tight control over arithmetic and collection ordering, and no ambient nondeterminism (GC ordering, randomised iteration, loose floating-point).
- **I2 / I4 — boundaries are machine-enforced.** The strongest realisation is one where the dependency direction is a *compile-time* fact, not a lint an agent can drift past.
- **I8 — dependency minimalism**, pinned, latest-stable, offline-installable, no heavy transitive trees.
- **I9 — one-command reproducible gate**, pinned toolchain, no cloud CI. Favours a single first-party toolchain (build + test + fmt + lint + lock).
- **I7 / ADR 0005 — Apple-Silicon MacBook, fully offline.** Confirmed dev/play machine: **M5 Max, 48 GB unified memory** — ample GPU and headroom for a strong local opponent model.

Additional forces fixed this session: the codebase is authored **entirely by agents** (no human code author — contract §1), so the compiler should be the safety net; the visual target is a **3D terrain mesh**, which requires a real GPU pipeline; and the LLM runtime is deliberately **out of scope here** — it lives behind `LLMOpponentPort` and is chosen in a later ADR, so it does not select the app language.

## Decision

We will use **Rust** as the single implementation language for the entire project — deterministic core, application layer, ports, and adapters.

- **Runtime:** a natively compiled `aarch64-apple-darwin` binary. No managed runtime, VM, or garbage collector. The **latest stable Rust**, **edition 2024**, pinned via `rust-toolchain.toml`; the exact version is pinned when the enforcement framework is built (contract §7).
- **Architecture as a Cargo workspace.** The ports-and-adapters graph of [`20-architecture.md`](../20-architecture.md) is realised as a workspace of crates so that the dependency rule (I2) is enforced by the compiler, not convention:
  - `core` — the deterministic simulation. **Zero external dependencies.** Cannot name any other project crate. Pure, `#![forbid(unsafe_code)]`, no I/O.
  - `config` — validated parameter types the `core` depends on (data only, no logic).
  - `ports` — port *interfaces* (traits) only.
  - `app` — orchestration; depends on `core`, `config`, and `ports`; never on a concrete adapter crate.
  - `adapters/*` — one crate per adapter (renderer, input, persistence, clock-rng, config-loader, llm, audio, logging); each depends on `ports` (to implement) plus external libraries. Adapters do not depend on one another.
  - a thin binary (composition root) that wires adapters to ports at startup.

  A `core` that lists no other crate in its `Cargo.toml` *cannot* import an adapter — the illegal direction fails to compile. This is the mechanism behind I2/I4.
- **Determinism discipline in the core** (enforced by the replay harness + lints during §7):
  - Prefer **integer / fixed-point** arithmetic for simulation math. Any floating-point use must be deterministic (no fast-math, stable evaluation order).
  - **No reliance on `HashMap`/`HashSet` iteration order** in the core; use `Vec`/`BTreeMap`/`BTreeSet` (or a fixed-seed hasher) so ordering is reproducible.
  - All randomness enters via a **seeded RNG** through the clock/RNG port; no `std` global RNG, no wall-clock, no I/O in `core`.
- **Gate tooling is `cargo`-native.** The §6.2 capabilities map onto: `rustc` + `clippy` (types + lint, zero-warning policy), `rustfmt` (canonical format), `cargo test` + coverage (e.g. `cargo-llvm-cov`), the crate graph + `cargo-deny` + a boundary lint (dependency/boundary checker), and a config-schema validator crate. The concrete tool versions are each recorded in the enforcement-framework ADR(s) and pinned; `Cargo.lock` + `rust-toolchain.toml` make the environment reproducible (I8, I9).

**Explicitly out of scope for this ADR** (each gets its own, per one-decision-per-ADR):
- **Rendering framework** (e.g. `wgpu` vs `bevy`) — a separate renderer ADR. Note: whatever is chosen is a renderer/input **adapter**; the pure `core` owns game state. `bevy`'s ECS `World` must **not** become the simulation state, or it breaks I3/I2.
- **LLM runtime & model** — a local Metal-backed runtime (e.g. Ollama / `llama.cpp` sidecar now, MLX possible later) behind `LLMOpponentPort`; the 48 GB machine makes the model choice generous but does not affect this decision.
- The **async executor** for adapters (LLM/persistence run off the critical path) — an adapter-layer dependency chosen later; the `core` stays synchronous and pure.

## Consequences

- **Positive:**
  - Compile-time boundary enforcement via the crate graph is the cleanest possible realisation of I2/I4 — boundaries can't erode silently across agent sessions.
  - Full control over determinism: no GC, fixed-point-friendly, deterministic collections, seeded RNG — directly serves I3 and the replay harness.
  - A single first-party toolchain (`cargo` for build/test/fmt/clippy) gives the one-command gate (I9); `Cargo.lock` + `rust-toolchain.toml` pin everything (I8/I9).
  - The strict compiler + `clippy` act as the safety net an agents-only codebase needs — whole classes of refactor errors fail at build time, not three sessions later.
  - Native `arm64` performance suits the interactive frame budget and a cheap simulation tick; `wgpu`/`bevy` provide a Metal-backed 3D path. The M5 Max makes Rust's compile times a non-issue.
- **Negative / trade-offs:**
  - Steeper than a scripting language for rapid prototyping; agents must apply borrow-checker discipline.
  - Determinism is not free: agents must avoid `HashMap` ordering and undisciplined float math in the `core` — enforced by lint + replay tests, but a standing hazard.
  - A 3D engine such as `bevy` drags a large transitive dependency tree; it must be confined to the adapter layer so the `core` honours I8. `bevy`'s ECS also tempts blurring the state boundary (see renderer note above).
  - Smaller game/GUI ecosystem than turnkey engines (Unity/Godot); more is built in-house.
- **Enforcement / gate impact:** the gate becomes a `cargo`-based one-command script; the dependency/boundary checker is realised as (a) the workspace crate graph, (b) `cargo-deny` (bans/licenses/advisories, offline), and (c) a boundary lint; the determinism/replay harness and coverage tooling are Rust crates. All are built and proven green on an empty project in §7 phase 1, with concrete tool choices recorded in their own ADR(s).
- **Docs to update (this change):** `decisions/README.md` (index + open list), `20-architecture.md` §5 (concrete crate layout now defined here). No invariant changes.

## Alternatives considered

- **Swift.** Best native-Mac integration: Metal rendering and in-process **MLX** for the local model. Rejected as the *whole-app* language: module access control is a weaker boundary mechanism than a crate graph + `cargo-deny`; determinism discipline falls entirely on the author; smaller cross-agent ecosystem. Its one decisive advantage (MLX) is capturable behind `LLMOpponentPort` regardless of app language, and Swift may still be used *inside* the LLM adapter later via ADR if in-process MLX is chosen.
- **TypeScript (Node/Bun + Tauri or a web renderer).** Fastest iteration, most agent fluency, trivial 3D via three.js. Rejected: npm's dependency culture fights I8; determinism is easy to break (async ordering, ecosystem looseness); it needs a managed runtime; and "agents-only" removes the human-readability argument that most favours it.
- **Go.** Simple, fast compile. Rejected: randomised map iteration and GC complicate bit-for-bit determinism; `internal` packages are a weaker boundary than the crate graph; thinner 3D ecosystem.
- **C# / .NET (Godot or MonoGame).** Strong engine tooling. Rejected: GC + managed runtime complicate determinism; adds an engine and the .NET runtime as dependencies; boundary enforcement is less clean than Cargo crates.
