# Documentation — god-game (working title)

This `docs/` folder is the **governing contract** for an AI-built, AI-maintained god-game inspired by *Populous II*, featuring a **local-LLM strategic opponent**, running **offline on a MacBook**. There is **no human code author**: these documents are the authority the code answers to, not the other way around.

**If you are an agent: read [§ Context Diet](#context-diet) first, then load only what your task needs.**

---

## The suite

| Doc | What it is | Read when |
|---|---|---|
| [`00-vision.md`](./00-vision.md) | The pitch, pillars, and non-goals. | Onboarding; design-intent questions. |
| [`10-game-design.md`](./10-game-design.md) | Concrete, parameter-referenced v1 mechanics (living design). | Any gameplay/mechanics/balance task. |
| [`20-architecture.md`](./20-architecture.md) | Ports-&-adapters around a deterministic core; dependency rules. | Any core/port/tooling task. |
| [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) | **The constitution.** Invariants, Definition of Done, workflow, enforcement, bootstrapping order. | **Always.** |
| [`40-parameterisation.md`](./40-parameterisation.md) | Config-driven design: taxonomy, mandatory dot-notation namespacing, validation, the no-code-change rule. | Any tunable/schema/balance task. |
| [`50-llm-opponent.md`](./50-llm-opponent.md) | The strategic-advisor LLM: port, observation/decision schemas, cadence, fallback, determinism. | Any opponent/AI task. |
| [`60-constraints.md`](./60-constraints.md) | MacBook-only, offline, budgets, dependency policy, reproducibility. | Environment/tooling/dependency/perf tasks. |
| [`70-glossary.md`](./70-glossary.md) | Definitions of domain & technical terms. | Any term you're unsure about. |
| [`decisions/`](./decisions/) | Architecture Decision Records — the only way to make architectural changes. | Making/looking up a significant decision. |
| [`contracts/`](./contracts/) | Reserved home for machine-readable schemas (added in the enforcement-first phase). | Schema work, once tools are chosen. |

**Suggested first read (humans & new agents):** `00` → `30` → `20` → `40` → `50`. `10`/`60`/`70` as needed.

---

## Context Diet

> **Rule:** load the **minimum** set of docs a task needs. Over-loading docs is *context bloat* — it dilutes attention and degrades output. Under-loading risks violating a rule you didn't read. The mapping below is the sanctioned middle path.

**Always loaded, for every task:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md). It is non-negotiable — the invariants and Definition of Done apply to all work.

Then load by task type:

| Task type | Load (in addition to the contract) | Explicitly skip |
|---|---|---|
| **Balance / tuning / content** (numbers, powers, scenarios) | `40-parameterisation.md`, `10-game-design.md` | `20`, `50`, `60`, `contracts/` |
| **Core simulation change** (rules, economy, terrain, powers logic) | `20-architecture.md`, `10-game-design.md` | `50`, `60`, `00` |
| **LLM / opponent change** (port, prompts, strategy, difficulty) | `50-llm-opponent.md`, `20-architecture.md` | `10` (unless mechanics change), `60` |
| **Port / adapter / boundary work** | `20-architecture.md`; the specific port's doc if it has one (`50` for the LLM port) | `10`, `00` |
| **Tooling / gate / environment / dependency** | `60-constraints.md`, `20-architecture.md` | `10`, `50` |
| **Schema / config-format work** | `40-parameterisation.md`, `contracts/README.md`, `60-constraints.md` | `10` (unless adding keys), `00` |
| **Any architectural decision** | the relevant doc above **+** `decisions/` (read the index, then related ADRs) + `decisions/template.md` | — |
| **Terminology lookup** | `70-glossary.md` (targeted) | everything else |

Notes:
- "Skip" means *don't load by default* — pull it in if the specific task turns out to touch it.
- When a task spans types (e.g. a new mechanic that needs new config), union the rows.
- If you find yourself wanting all docs, the task is probably too big — split it (contract §4).

---

## Status

Documentation and environment stack decided (ADRs [0006](./decisions/0006-rust-language-and-runtime.md)–[0010](./decisions/0010-branch-workflow-and-ci.md)), and **Phase 1 (contract §7.1–7.2) is built**: the Cargo workspace realises the architecture as a crate graph, and the one-command gate — `cargo gate` — runs format, clippy, boundary, magic-number, schema-drift, config-validity, key-integrity, cargo-deny, and tests+coverage checks, green on the placeholder codebase. CI runs the same gate on every PR (ADR 0010). Next per §7.4: the config/parameter layer (Phase 2), then the deterministic core (Phase 3).

**Planning & backlog live on GitHub** ([ADR 0012](./decisions/0012-project-management-on-github.md)): tasks and bugs are [Issues](https://github.com/eggman0131/Providence/issues), the roadmap / "what's next" is the [Project board](https://github.com/eggman0131/Providence/projects), and *decisions* remain ADRs here. Both formerly-open `adr-needed` items are now decided — **LLM runtime & model** ([#8](https://github.com/eggman0131/Providence/issues/8)) by [ADR 0014](./decisions/0014-ollama-local-llm-runtime.md), and the **debug/HUD UI** ([#9](https://github.com/eggman0131/Providence/issues/9)) by [ADR 0015](./decisions/0015-debug-hud-ui-layer.md).
