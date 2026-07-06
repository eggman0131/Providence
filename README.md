# God — an AI-built god-game (working title)

A **single-player god-game for the Mac**, inspired by *Populous II*, in which you shape the
land to grow your followers and outwit a **rival deity driven by a local LLM**. You never
control units directly — you sculpt terrain, spend the faith of your worshippers as divine
power, and bend the world toward your people while a thinking opponent does the same against
you. Everything runs **locally and offline** on a single MacBook, and the entire codebase is
**built and maintained by AI agents** under a strict contract.

## What's unusual

- **The opponent thinks.** A local language model acts as a *strategic advisor* to a
  deterministic engine — an opponent with intent, not a fixed script.
- **No human writes the code.** Agents author and maintain everything; the documentation
  *is* the governance ([the contract](docs/30-ai-agent-contract.md) is its constitution).
- **Tunable to the core.** Balance, behaviour, and content are data, not code.

## Status

**Phase 1 (enforcement-first) complete** — the docs, the environment stack, and the
one-command gate exist; no gameplay code yet, exactly as the contract's bootstrapping
order demands. `cargo xtask setup` provisions the pinned toolchain; `cargo gate` runs
every enforcement check (format, lint, boundaries, magic numbers, schema drift, config
validity, key integrity, dependency policy, tests + coverage) and is the single
definition of "green", locally and in CI.

| Concern | Decision | ADR |
|---|---|---|
| Language + runtime | Rust — native arm64, Cargo workspace, `no_std` core | [0006](docs/decisions/0006-rust-language-and-runtime.md) |
| 3D rendering | `wgpu` / Metal, as a renderer adapter | [0007](docs/decisions/0007-wgpu-rendering-framework.md) |
| Config + schema | TOML, types-first (`serde`/`garde`/`schemars`) | [0008](docs/decisions/0008-toml-config-format-types-first-schema.md) |
| Enforcement + gate | `cargo gate` (xtask), custom boundary/replay/magic-number checks | [0009](docs/decisions/0009-enforcement-tooling-and-the-gate.md) |

Open: the **LLM runtime & model**, and a possible debug/HUD UI layer.

## Documentation

Everything lives in [`docs/`](docs/). Start with the [vision](docs/00-vision.md) and the
[AI-agent contract](docs/30-ai-agent-contract.md); significant decisions are recorded as
[ADRs](docs/decisions/). Agents: read the [Context Diet](docs/README.md#context-diet) first
and load only what a task needs.

## Requirements

Apple Silicon MacBook, macOS. Fully offline at runtime — no accounts, no network, no cloud.
