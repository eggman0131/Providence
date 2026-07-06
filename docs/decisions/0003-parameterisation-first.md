# 0003 — Parameterisation-first (no behavioural constants in code)

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../40-parameterisation.md`](../40-parameterisation.md), [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (I1)

## Context

The game must be tunable and re-themable "without code changes" — a founding requirement. In an author-less codebase this also improves safety: changing a number should not require an agent to touch (and risk) source code, and balance work should be a low-blast-radius, well-typed data change.

## Decision

All tunable behaviour, balance, and content lives in **versioned, schema-validated configuration** (invariant I1). Behavioural literals in source are defects ("magic numbers"). Configuration keys use a **mandatory hierarchical dot-notation** under **registered namespace roots** (`meta`, `sim`, `content`, `ai`, `render`, `input`, `runtime`); the validator rejects keys outside them. The core reads config only as injected immutable data (it never reads files).

## Consequences

- **Positive:** re-tuning/re-theming with no code edits; collision-free, self-locating keys; low-risk balance changes; content packs possible; clear config/code separation.
- **Negative / trade-offs:** upfront schema + validation machinery; discipline to route new tunables through config; a magic-number check to maintain.
- **Enforcement / gate impact:** requires a schema validator (with namespace enforcement), a magic-number conformance scan, a content-only-change test, and key-reference integrity checks — all part of the enforcement-first phase (contract §7).
- **Docs to update:** `40-parameterisation.md` (rules), `10-game-design.md` (keys), `50-llm-opponent.md` (`ai.*`).

## Alternatives considered

- **Constants with occasional config:** the usual drift-to-hardcoding; fails the founding requirement. Rejected.
- **Flat key names:** simpler but collision-prone across many agent sessions. Rejected in favour of namespaced dot-notation.
- **Choosing the concrete file format now (TOML/YAML/JSON):** deferred to the environment discussion; only the *requirements* are fixed here.
