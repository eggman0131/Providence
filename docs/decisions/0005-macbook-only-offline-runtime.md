# 0005 — MacBook-only, offline runtime

- **Status:** Accepted
- **Refined by:** [0010](./0010-branch-workflow-and-ci.md) — "no cloud CI is assumed" means the gate is complete and authoritative *locally*; it does **not** forbid CI. CI is endorsed as a required merge gate that runs **only** the canonical `cargo gate` (so local ≡ CI), and is the execution venue for changes not verified locally (Dependabot, cloud agents).
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../60-constraints.md`](../60-constraints.md), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (I7, I8, I9), [`0010`](./0010-branch-workflow-and-ci.md)

## Context

The game is for personal use on the director's MacBook, with a **local** LLM opponent. Committing to a single target removes portability tax, and committing to offline operation keeps the game private and dependency-light. These constraints shape the LLM runtime, dependency, and tooling choices, so they are fixed before those choices are made.

## Decision

The **only** supported runtime target is a **MacBook (Apple Silicon assumed as baseline)**, running **fully offline** (invariant I7). No runtime network dependency, no accounts, no telemetry; the LLM runs locally. Network use is permitted only at development time. Dependencies are minimised, pinned, and — when added — chosen at their latest stable version and recorded (I8). The environment is reproducible via one-command setup and one-command gate; no cloud CI is assumed (I9).

## Consequences

- **Positive:** no portability tax; private and self-contained; smaller dependency surface; the local model choice is unconstrained by server concerns.
- **Negative / trade-offs:** no other platforms without a future ADR; local model must fit MacBook resources (size/quantisation trade-offs); "latest stable" dependencies need periodic re-evaluation.
- **Enforcement / gate impact:** the gate runs locally as the sole authority; dependency additions should be checked for offline-compatibility and recorded; performance/memory budgets (`render.*`, `ai.llm.*`) tracked.
- **Docs to update:** `60-constraints.md` (limits), `50-llm-opponent.md` §7 (local runtime), `30-ai-agent-contract.md` (I7–I9).

## Alternatives considered

- **Cross-platform from the start:** large, permanent tax for zero benefit to a single-user Mac game. Rejected.
- **Cloud/hosted LLM opponent:** simpler model ops, but breaks offline/privacy (I7) and adds a hard runtime network dependency. Rejected.
- **Intel-Mac support:** possible later via ADR; not baseline.
