# 00 — Vision

> **Status:** Active · **Audience:** everyone (agents & the human director) · **Load this doc for:** onboarding and any design-intent question.

## Elevator pitch

A **single-player god-game for the Mac**, inspired by *Populous II*, in which you shape the land to grow your followers and outwit a **rival deity driven by a local LLM**. You never control units directly — you are a god: you sculpt terrain, spend the faith of your worshippers as divine power, and bend the world toward your people while a thinking opponent does the same against you. Everything runs **locally and offline** on a single MacBook, and the entire codebase is **built and maintained by AI agents** under a strict contract.

## What makes this project unusual

1. **The opponent thinks.** The enemy god's strategy comes from a local language model acting as a *strategic advisor* to a deterministic engine — an opponent with intent and adaptability, not a fixed script. See [`50-llm-opponent.md`](./50-llm-opponent.md).
2. **No human writes the code.** Agents (Opus / Sonnet) author and maintain everything. The documentation *is* the project's governance; [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) is its constitution.
3. **Tunable to the core.** Balance, behaviour, and content are data, not code — the game can be re-tuned and re-themed without programming. See [`40-parameterisation.md`](./40-parameterisation.md).

## Design pillars

- **The land is the game.** Terrain-shaping is the primary verb; almost everything else follows from it.
- **Faith is power.** Followers generate the resource; the resource buys divine intervention; intervention grows (or protects) followers. A legible loop.
- **A worthy, legible opponent.** The rival should feel intentional and beatable-but-challenging; its reasoning should be inspectable (record–replay), never opaque magic.
- **Deterministic and reproducible.** Same seed, same story. This is a hard technical pillar (I3), not just a nicety — it is what keeps an AI-maintained game debuggable.
- **Local, private, offline.** No servers, no accounts, no network at runtime.
- **Maintainable by machines.** Small modules, explicit boundaries, enforced by tooling — so the next agent can safely extend it. See [`20-architecture.md`](./20-architecture.md).

## Non-goals (for now)

- Multiplayer / online play.
- Cross-platform support (MacBook-only — see [`60-constraints.md`](./60-constraints.md)).
- A faithful 1:1 clone of Populous II — we take inspiration, then design our own coherent mechanics ([`10-game-design.md`](./10-game-design.md)).

These non-goals can be revisited via an ADR if intent changes.
