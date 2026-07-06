# 0014 — Ollama as the local LLM runtime for the strategic-advisor opponent

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Director + agent session
- **Related:** [`../50-llm-opponent.md`](../50-llm-opponent.md) §7, [`../60-constraints.md`](../60-constraints.md), [`../40-parameterisation.md`](../40-parameterisation.md), [`0002`](./0002-llm-as-strategic-advisor.md), [`0004`](./0004-deterministic-core-ports-and-adapters.md), [`0005`](./0005-macbook-only-offline-runtime.md), [`0008`](./0008-toml-config-format-types-first-schema.md), [`0007`](./0007-wgpu-rendering-framework.md); resolves issue [#8](https://github.com/eggman0131/providence-legacy/issues/8)

## Context

[ADR 0002](./0002-llm-as-strategic-advisor.md) fixed the opponent as a **strategic advisor** behind a single `LLMOpponentPort`, and [ADR 0005](./0005-macbook-only-offline-runtime.md) fixed a **Mac-only, offline** runtime with the model running locally. [`50-llm-opponent.md`](../50-llm-opponent.md) §7 deliberately deferred the concrete runtime/model to "the environment discussion" and an ADR, requiring only that a swap need "a new adapter + config, never a change to the core." This is that ADR (issue [#8](https://github.com/eggman0131/providence-legacy/issues/8)).

Forces that bound the choice:

- **Offline & private (I7):** no runtime network dependency, no account, no telemetry — the model runs locally on Apple Silicon ([ADR 0005](./0005-macbook-only-offline-runtime.md), [`60-constraints.md`](../60-constraints.md) §2).
- **Fits the machine (`60-constraints.md` §3):** the model must sit comfortably in MacBook RAM alongside the game; size/quantisation is an `ai.llm.*` decision, not a code constant.
- **Agents-only maintenance (contract §1):** as with [ADR 0007](./0007-wgpu-rendering-framework.md), *how well models can author and maintain the adapter* is a first-class criterion — favour a small, well-understood, low-churn integration surface.
- **Determinism untouched (I3):** the LLM already lives **outside** the determinism boundary ([ADR 0002](./0002-llm-as-strategic-advisor.md)/[0004](./0004-deterministic-core-ports-and-adapters.md)); reproducibility comes from record–replay, not model determinism ([`50-llm-opponent.md`](../50-llm-opponent.md) §6).
- **Dependency freshness/pinning (I8):** prefer latest stable, confirm it runs offline, pin it, record it.

Decisive additional fact: **Ollama is already a proven local dependency in this repo at dev time** — both the graphify semantic re-extract (post-commit hook) and the doc-drift review ([ADR 0013](./0013-advisory-doc-drift-review-on-push.md)) drive Ollama with `qwen3.6:35b-mlx`. Adopting it for the game runtime consolidates the whole project onto **one** local inference stack rather than introducing a second.

## Decision

We will use **Ollama** as the local LLM runtime for the strategic-advisor opponent, implemented in a dedicated **`llm-ollama` adapter crate** that realises `LLMOpponentPort`, running fully offline on Apple Silicon.

- **The model is pluggable via config, not code.** The concrete model is a named Ollama tag under the reserved `ai.llm.*` namespace (`ai.llm.runtime = "ollama"`, `ai.llm.model = "gemma4:26b-mlx"`). The **baseline reference model is `gemma4:26b-mlx`**; the exact model and quantisation may change **without a new ADR** (it is tunable content per I1). Only switching the *runtime itself* requires a new adapter — honouring [`50-llm-opponent.md`](../50-llm-opponent.md) §7.
- **Loopback, not the network.** The adapter speaks Ollama's **local HTTP API over loopback** (`127.0.0.1`, default port `:11434`). This is IPC to a co-resident daemon — **not** a runtime network dependency (I7): no external host, no internet, no account, no telemetry. The HTTP/JSON client dependency is **confined to the `llm-ollama` adapter crate**; the boundary checker and scoped `cargo-deny` keep it (and its transitive tree) out of `core`/`config`/`ports`/`app`, exactly as `wgpu` is confined in [ADR 0007](./0007-wgpu-rendering-framework.md).
- **Ollama is a *soft* runtime dependency.** If the daemon or model is missing, slow, or errors, the adapter degrades to the **deterministic fallback strategy** ([`50-llm-opponent.md`](../50-llm-opponent.md) §5). The game stays fully playable with the scripted opponent — *Ollama down ≠ game down*. The daemon is required only for the *intelligent* opponent, not to launch the game.
- **The gate never requires Ollama.** Unit and replay tests use the scripted/mock `LLMOpponentPort` double; live Ollama is exercised only at dev time and by the `/verify` end-to-end step. This keeps `cargo gate` offline and reproducible (I9).

**Out of scope for this ADR:** the `Observation`/`StrategyDecision` schema and prompt design ([`50-llm-opponent.md`](../50-llm-opponent.md) §2, §4 — future phase-5 work); the exact quantisation, context length, temperature/seed, and cadence values (all `ai.llm.*` config per I1); and the model-pinning mechanics in setup (an environment task, below).

## Consequences

- **Positive:**
  - **One inference stack for the whole project.** Ollama already runs the graphify and doc-drift tooling here, so the game introduces **no new class of dependency** — the environment story is consolidated and already exercised.
  - **Thin, model-friendly adapter.** A small HTTP/JSON client behind the port is precisely the well-understood, low-churn code an agents-only codebase maintains reliably (the same competence criterion that motivated [ADR 0007](./0007-wgpu-rendering-framework.md)).
  - **Model management is decoupled.** Pulling and swapping models is Ollama's job, addressed by a config tag; changing the model is therefore **pure config**, satisfying the `50-llm-opponent.md` §7 pluggability requirement.
  - **Fits the machine.** `gemma4:26b-mlx` sits within the MacBook memory budget alongside the game ([`60-constraints.md`](../60-constraints.md) §3); quantisation remains an `ai.llm.*` knob if headroom gets tight.
  - **Offline & private preserved** (loopback only, no external network — I7) and **graceful degradation already specified** (daemon down → scripted fallback).
- **Negative / trade-offs:**
  - The game now assumes an **installed, running Ollama daemon** for the intelligent opponent — a local *process* prerequisite (provisioned by `cargo xtask setup`). Without it the opponent silently falls back to scripted.
  - **Version drift.** Ollama and the model must be pinned and periodically re-evaluated (I8). The `-mlx` tag is an Ollama-side convention (as already used for `qwen3.6:35b-mlx`), not portable off this stack.
  - **A process hop.** An out-of-process HTTP round-trip adds latency versus in-process inference — bounded by the `ai.llm.*` timeout and kept off the simulation's critical path ([`60-constraints.md`](../60-constraints.md) §3, [`50-llm-opponent.md`](../50-llm-opponent.md) §5).
- **Enforcement / gate impact:**
  - New **`llm-ollama` adapter crate**: the boundary checker keeps it out of the core, and `cargo-deny` is scoped so the HTTP client and its transitive tree cannot leak into `core`/`config`/`ports`/`app` (mirrors the `wgpu` confinement in [ADR 0007](./0007-wgpu-rendering-framework.md)).
  - The **gate must not require a running Ollama daemon**: tests use the scripted/mock adapter; live Ollama stays a dev-time / `/verify` concern, keeping the gate offline and reproducible (I9).
  - `ai.llm.runtime` and `ai.llm.model` join the config **types + schema** when the adapter lands (phase 5), under the **existing** `ai.llm.*` root — no new namespace root, so no separate ADR for the keys ([ADR 0008](./0008-toml-config-format-types-first-schema.md)).
  - `cargo xtask setup` gains an **advisory, skip-if-absent** step ensuring Ollama and the baseline model are present — consistent with how the doc-review hook already tolerates Ollama being down.
- **Docs to update (this change):** `decisions/README.md` (index), `50-llm-opponent.md` §7 (runtime now concretely Ollama), `20-architecture.md` (`LLMOpponentPort` adapter row names Ollama), `60-constraints.md` §3 (memory bullet points to this ADR), `40-parameterisation.md` §3 (`ai.llm.*` concrete runtime/model keys), `CLAUDE.md` (bootstrapping note: #8 resolved). No invariant changes.

## Alternatives considered

- **Candle (pure-Rust, in-process inference).** No external daemon, tightest integration, no loopback hop — architecturally the purest fit. But it makes *us* own model loading, quantisation, format plumbing, and Apple-Silicon performance tuning; the maintenance surface is larger for an agents-only project, and it would run a **second** inference stack alongside the Ollama the repo already uses. Attractive enough to revisit via a future ADR if the daemon prerequisite becomes a burden. Rejected for v1 on maintenance cost and stack duplication.
- **llama.cpp directly (or Rust bindings).** Maximum control and no daemon, but more build/integration complexity and manual model management. Ollama is essentially a managed wrapper over the same lineage with a stable local API and a model registry; the wrapper's ergonomics win for an agents-only codebase. Rejected.
- **MLX / `mlx-lm` directly (Apple's framework).** Best raw Apple-Silicon performance, but its first-class path is **Python** — a heavyweight non-Rust runtime dependency at the adapter — and the Rust story is thin. Ollama already exposes MLX-tagged models here (the `…-mlx` tags), capturing much of the benefit without the Python runtime. Rejected as a direct dependency.
- **Cloud/hosted LLM.** Already rejected by [ADR 0005](./0005-macbook-only-offline-runtime.md) (breaks offline/privacy I7, adds a hard runtime network dependency). Not reconsidered.
