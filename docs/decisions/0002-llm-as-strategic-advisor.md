# 0002 — LLM opponent as a strategic advisor, not an actuator

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../50-llm-opponent.md`](../50-llm-opponent.md), [`../20-architecture.md`](../20-architecture.md), [`0004`](./0004-deterministic-core-ports-and-adapters.md)

## Context

The rival deity must feel intelligent, but the simulation core must remain **deterministic and reproducible** (invariant I3). A non-deterministic LLM directly mutating game state would make the game unreproducible and untestable, and would let malformed model output corrupt state.

## Decision

The local LLM acts as a **strategic advisor**. It receives a compact, structured **Observation** and returns a schema-validated **StrategyDecision** (declarative intent). A **deterministic translator/validator** inside the boundary converts that intent into concrete, *legal* commands, which the core applies. The LLM never emits raw engine mutations and never touches state directly. It sits **outside** the determinism boundary behind a single `LLMOpponentPort` with a scripted test double.

## Consequences

- **Positive:** intelligent, adaptable opponent *and* a deterministic core; the LLM cannot corrupt state (illegal intent is dropped); testable via mock adapters; reproducible via record–replay; opponent behaviour is tunable content (`ai.*`).
- **Negative / trade-offs:** an intent→command translation layer must be designed and maintained; the opponent is only as good as the strategy vocabulary and translator.
- **Enforcement / gate impact:** requires a strict response-schema validator, a deterministic fallback strategy, and record–replay tests; boundary checker must keep the LLM adapter out of the core.
- **Docs to update:** `50-llm-opponent.md` (full design), `20-architecture.md` (boundary), `40-parameterisation.md` (`ai.*`).

## Alternatives considered

- **LLM issues concrete per-turn actions (full tactical control):** most direct, but hard to bound/test and endangers determinism. Rejected for v1.
- **Hybrid (LLM strategy + tactical actions, tunable split):** more flexible but more complex to specify; deferred — may revisit via ADR.
- **Scripted/behaviour-tree AI only:** deterministic and simple but not the "intelligent opponent" this project is about. Retained instead as the **fallback** strategy.
