# 0004 — Deterministic core with ports & adapters

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../20-architecture.md`](../20-architecture.md), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (I2, I3, I4), [`0002`](./0002-llm-as-strategic-advisor.md)

## Context

The project must stay supportable and enhanceable by AI agents over many sessions, and must support an intelligent (non-deterministic) opponent without becoming unreproducible or untestable. Both goals point to the same structure: isolate a pure, deterministic simulation and push all side effects to swappable edges.

## Decision

Adopt a **hexagonal (ports-and-adapters)** architecture around a **pure, deterministic core**:
- The **core** is I/O-free, uses only a seeded RNG, and depends only on injected config data. Same seed + inputs ⇒ identical output (I3).
- All side effects (LLM, renderer, input, persistence, clock/RNG source, config, audio, logging) are reached only via **ports** (interfaces), implemented by **adapters**, each with a test double (I4).
- Dependencies point **inward** only; no cycles (I2). The boundary checker enforces this.

## Consequences

- **Positive:** the core is trivially testable and reproducible; adapters (incl. LLM runtime) are swappable via config; boundaries are machine-enforced, protecting the design across agent sessions; supports record–replay.
- **Negative / trade-offs:** more indirection than a monolith; a composition root and dependency injection are required; ports must be designed deliberately.
- **Enforcement / gate impact:** requires the dependency/boundary checker, a determinism/replay test harness, and coverage thresholds (core highest) — all in the enforcement-first phase (contract §7).
- **Docs to update:** `20-architecture.md` (structure), `30-ai-agent-contract.md` (I2–I4), `50-llm-opponent.md` (LLM as an outside-the-boundary port).

## Alternatives considered

- **Layered monolith without strict ports:** simpler initially, but boundaries erode without enforcement and the LLM/determinism split becomes muddy. Rejected.
- **Entity-Component-System-only design:** an option for the *core's internal* organisation, not a substitute for the boundary architecture; can be adopted internally later via ADR without changing this decision.
