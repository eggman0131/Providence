# 60 — Constraints

> **Status:** Active · **Governed by:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) (I7, I8, I9) · **Load this doc for:** tooling/environment, dependency, and performance tasks.

Hard limits on where and how this project runs. These bound every technical choice; the tools/environment discussion happens *inside* these constraints.

---

## 1. Target platform: a single MacBook

- The **only** supported target is a MacBook. **Apple Silicon is assumed** as the baseline (to be confirmed/pinned in the environment ADR); this influences the local LLM runtime choice.
- No other OS, no cross-platform obligation, no mobile, no console. Simplifying for one machine is a feature, not a limitation.
- Do not add abstractions purely for portability we will never use. (Ports exist for *testability and boundaries* per I4, not for OS portability.)

## 2. Offline & private (I7)

- **No runtime network dependency.** The game — including the LLM opponent — runs fully offline. The model runs **locally**.
- No accounts, no telemetry, no phone-home, no cloud services required to play.
- Network use is acceptable only at **development time** (e.g. fetching a dependency, researching a version) — never as a runtime requirement.

## 3. Performance & resource budgets

Concrete numbers are config/environment-ADR decisions; the *requirements* are:
- **Interactive frame rate** for terrain shaping and presentation (`render.*` budget).
- **Bounded LLM latency** off the simulation's critical path — the opponent's thinking must never stall the game; a missed budget triggers the deterministic fallback (see [`50-llm-opponent.md`](./50-llm-opponent.md) §5).
- **Memory headroom for a local model.** The chosen model must fit comfortably in a MacBook's RAM alongside the game; the runtime is **Ollama** and the baseline model `gemma4:26b-mlx` ([ADR 0014](./decisions/0014-ollama-local-llm-runtime.md)), with size/quantisation remaining an `ai.llm.*` knob.
- The deterministic core must be efficient enough that a full tick is cheap relative to the frame budget.

## 4. Dependency policy (I8)

- **Prefer zero dependencies.** Add one only when it clearly beats writing/maintaining the code ourselves.
- When adding one: **research and prefer the latest stable version**, confirm it runs on the target offline, **pin** it, and record it (ADR or changelog) with version + rationale.
- Avoid dependencies that require network access at runtime, phone home, or drag in heavy transitive trees.
- Re-evaluate/upgrade deliberately; do not let pinned versions rot.

## 5. Reproducible environment (I9)

- **One command** provisions the dev environment; **one command** runs the full gate (contract §9).
- The toolchain (language version, tools, model runtime) is **pinned**.
- **No cloud CI is *required*.** The gate is complete and runnable locally — no correctness check exists *only* in the cloud. CI, when present, runs **only** the canonical gate (never bespoke cloud-only checks) and is the *required execution venue* for changes not verified locally (e.g. Dependabot or cloud-agent PRs). The **gate — wherever it runs — is the arbiter of "green".** See [ADR 0010](./decisions/0010-branch-workflow-and-ci.md).
- Because there is no human author, "works on my machine" is meaningless — reproducibility is the only guarantee of correctness across agent sessions.

---

Any change to these constraints (e.g. supporting Intel Macs, allowing an optional online feature) is **architectural** and requires an ADR.
