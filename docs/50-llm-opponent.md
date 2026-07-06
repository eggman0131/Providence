# 50 — The LLM Opponent (Strategic Advisor)

> **Status:** Active · **Governed by:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) (I3, I4) and [`20-architecture.md`](./20-architecture.md) · **Load this doc for:** any change to the rival-deity AI, the `LLMOpponentPort`, or `ai.*` parameters.

The rival deity is driven by a **local LLM acting as a strategic advisor** — not as a puppeteer. This design choice (recorded in [ADR 0002](./decisions/0002-llm-as-strategic-advisor.md)) is what lets an intelligent, non-deterministic opponent coexist with a **deterministic, reproducible core** (I3).

---

## 1. Role: advisor, not actuator

The LLM's job is to decide the enemy god's **intent** — high-level strategy, priorities, and posture. It does **not** mutate the world, and it does **not** emit raw engine commands. Instead:

```
observation ──► LLM ──► STRATEGY (intent) ──► deterministic translator ──► candidate commands ──► core validates & applies
             (outside determinism boundary)                              (inside boundary)
```

The deterministic engine turns strategy into concrete, **legal** actions and executes them. If the LLM proposes something impossible or illegal, the engine simply does not do it — the LLM cannot corrupt game state. This keeps the core pure while the "brain" stays flexible.

---

## 2. The port contract (`LLMOpponentPort`)

A single interface, injected into the application layer, with a real adapter and a test double.

```
LLMOpponentPort:
    decide(observation: Observation) -> StrategyDecision
```

### 2.1 Observation (engine → LLM)
A **compact, structured, deterministic** snapshot of what the enemy god can "know", built by the application from core state. It is *derived data*, never a live reference to core state. Properties:
- Small and stable-shaped (summary, not the raw world) — respects local-model context limits and latency budgets.
- Contains only information the opponent is allowed to act on (supports fog-of-war / difficulty handicaps via `ai.difficulty.*`).
- Deterministically serialisable, so a given state always yields the same observation.

### 2.2 StrategyDecision (LLM → engine)
A **strictly schema-validated** structure describing intent from the `ai.strategy.*` vocabulary — for example: an overall posture (expand / harass / fortify / all-out), ranked goals, target regions/settlements, and a desired power-usage emphasis. It is **declarative** (what to pursue), never imperative (exact tiles to modify). The engine is free to realise the intent however the rules allow.

### 2.3 Translation & validation (inside the boundary)
The application/core translates a `StrategyDecision` into candidate commands, then the core validates each for legality against current state and `sim.*` params before applying. Illegal or unaffordable candidates are dropped, not forced.

---

## 3. Decision cadence

The opponent does not think every tick. Cadence is parameterised:
- `ai.llm.decision.cadence_ticks` — solicit a new strategy every N ticks, and/or
- event triggers (e.g. under attack, resource threshold crossed) — also parameterised.

Between decisions, the engine keeps executing the *current* strategy deterministically. This bounds LLM calls (cost/latency) and keeps behaviour coherent.

---

## 4. Prompt architecture (parameterised)

Prompts are **content/config, not code** (I1). Under `ai.llm.*` / `ai.strategy.*`:
- **System role** — who the deity is, the rules of engagement, and the **required output schema**.
- **State serialisation** — how the `Observation` is rendered into the prompt.
- **Strategy library / few-shot** — the catalogue of allowed strategies and worked examples.
- **Response schema** — the exact structure the model must return.

Because these are parameters, its "personality" and tactics are tunable without code changes, and multiple opponent profiles can ship as content packs.

---

## 5. Robustness (never crash the game)

The LLM is untrusted and may be slow, unavailable, or produce malformed output. The adapter must degrade gracefully:
- **Strict parsing** — the response is validated against the schema; anything non-conforming is rejected.
- **Deterministic fallback** — on invalid output, timeout, or model unavailability, the engine falls back to a **deterministic default strategy** (a scripted baseline). The game continues; the opponent is never "stuck".
- **Budgets** — `ai.llm.*` sets a timeout and runs the call off the simulation's critical path (async); a missed budget triggers the fallback.
- **Bounded influence** — even a valid decision only *proposes* intent; §2.3 legality checks are the backstop.

---

## 6. Determinism & testing

The LLM is non-deterministic, so it lives **outside** the determinism boundary (I3). To keep the *game* reproducible and testable (I5):
- **Test doubles first.** Nearly all tests use a **scripted/mock adapter** that returns fixed `StrategyDecision`s. Core and application logic are fully tested without ever invoking a model.
- **Record–replay.** A session can record the resolved decisions (and/or raw LLM outputs). Replaying feeds those recorded decisions back through the deterministic path *without* calling the model, reproducing the session bit-for-bit — invaluable for debugging and regression tests.
- **Seed & temperature** — `ai.llm.*` exposes temperature and any sampling seed the runtime supports, for as-reproducible-as-possible live runs; true reproducibility comes from record–replay, not from trusting model determinism.

---

## 7. Local runtime (pluggable)

Per I7, the model runs **locally on the MacBook, offline**. The runtime is **Ollama** ([ADR 0014](./decisions/0014-ollama-local-llm-runtime.md)), driven through the single `LLMOpponentPort` by an `llm-ollama` adapter that speaks Ollama's local HTTP API over **loopback** (no external network — loopback IPC to a co-resident daemon is not a runtime network dependency). The **model is pluggable via `ai.llm.*` config**, a named Ollama tag (`ai.llm.model`); the baseline reference model is **`gemma4:26b-mlx`**, and the exact model/quantisation may change without an ADR (tunable content per I1). If the daemon or model is unavailable, the adapter degrades to the deterministic fallback (§5), so **Ollama down ≠ game down**. Swapping the *runtime itself* still requires **only** a new adapter + config, never a change to the core or application.

---

## 8. Difficulty (parameterised)

Difficulty is expressed in `ai.difficulty.*`, not in code — e.g.:
- `ai.difficulty.strategy_trust` — how fully the engine pursues the LLM's proposed intent.
- resource handicaps/bonuses for the opponent.
- `ai.llm.decision.cadence_ticks` — how often the opponent re-thinks (faster = sharper).
- how much of the world the observation reveals (fog handicap).

This lets difficulty be tuned — and new opponent profiles authored — entirely as content, consistent with [`40-parameterisation.md`](./40-parameterisation.md).
