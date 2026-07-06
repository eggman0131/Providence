# 0011 — Advisory (non-gating) code scanning with CodeQL

- **Status:** Proposed
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§6, §9; I8, I9), [`../60-constraints.md`](../60-constraints.md) (§5), [`0005`](./0005-macbook-only-offline-runtime.md), [`0009`](./0009-enforcement-tooling-and-the-gate.md), [`0010`](./0010-branch-workflow-and-ci.md)

## Context

The repository is public on GitHub, which offers **code scanning** (CodeQL → Security-tab alerts) at no cost. It is attractive as defense-in-depth. But GitHub code scanning is inherently a **cloud-only** mechanism, and [ADR 0010](./0010-branch-workflow-and-ci.md) §2 is emphatic that CI "runs **only** the canonical gate — no bespoke CI-only checks," because **I9** forbids a "separate cloud CI to hide behind."

The tension resolves on one distinction: I9 forbids a cloud-*exclusive* **gate** — correctness that decides mergeability but cannot be reproduced on the MacBook. It does not forbid cloud-*exclusive* **reporting**. The `cargo gate` already performs the SAST-equivalent work locally — `clippy -D warnings`, `cargo-deny` (advisories/CVEs, licenses, bans, sources), a `#![no_std]` + `#![forbid(unsafe_code)]` core (ADR 0009) — so a scanner adds *breadth of reporting*, not a new *authority*.

Repo-level Dependabot **alerts** and **security updates** are already enabled; those, plus the gate's `cargo-deny`, cover the dependency-vulnerability surface. This ADR concerns first-party *source* scanning.

## Decision

We will run **CodeQL as an advisory, non-gating code scanner**, and keep the gate the sole arbiter of correctness.

1. **Non-gating, always.** CodeQL publishes to the Security tab only. It **must never** be a required status check on `main`. Mergeability remains defined solely by `cargo gate` (ADR 0009/0010). I9 stays literally true — no cloud-*exclusive* correctness gate exists.
2. **Reporting, not the gate.** `.github/workflows/codeql.yml` is separate from the `gate` workflow. It runs on **push to `main`** and a **weekly schedule** (plus manual dispatch), deliberately **not** on every pull request — the gate already covers each PR, and omitting the PR trigger keeps CodeQL visibly advisory and cheap.
3. **Advanced, not default, setup.** GitHub's "default setup" is disabled; the workflow file (advanced setup) is used instead so triggers, runner, permissions, languages, and build-mode are under version control and reviewable. The two setups are mutually exclusive.
4. **Linux runner.** CodeQL runs on `ubuntu-latest`, not the macOS target. It is static analysis of *source* with no build, runtime, or determinism concern, so the `aarch64-apple-darwin` target-match rule ([ADR 0005](./0005-macbook-only-offline-runtime.md)/[0006](./0006-rust-language-and-runtime.md)) — which exists for the *gate* and the *game* — does not apply, and Linux keeps CI minutes cheap.
5. **Languages & extraction.** Scans `rust` (first-party source) and `actions` (the workflow files themselves), both with `build-mode: none` (source-based extraction; no `cargo build` in the scan). Default (security) query suite; wider suites are a non-ADR tuning knob.
6. **Offline integrity (I7).** CodeQL is a **development-/CI-time** tool only; it is never a runtime dependency of the game, which stays fully offline.

## Consequences

- **Positive:** Security-tab defense-in-depth beyond clippy/`cargo-deny`, at no cost on a public repo; I9 preserved by construction (advisory reporting, never a gate); cheap and clearly non-authoritative (Linux, off the PR path). Scanning `actions` also lints the workflow files (it flagged the `GITHUB_TOKEN` over-permission in `ci.yml`, now fixed).
- **Negative / trade-offs:** a second, cloud-only workflow now exists (mitigated: it is not a gate and is documented as such); CodeQL's Rust support is relatively new/less mature than for older languages, so signal will improve over time; findings must be triaged by the director, not the gate.
- **Enforcement / gate impact:** **none.** No new gate check; `cargo gate` is unchanged and remains the single definition of "green." Branch protection's required check stays `gate` only — adding CodeQL to required checks would violate this ADR and I9.
- **Docs to update:** `decisions/README.md` (index + open list). No invariant *values* change; ADR 0010 is not rewritten — this ADR refines its scope by distinguishing advisory reporting from the gate.

## Alternatives considered

- **CodeQL as a required gate.** Maximal GitHub-native SAST, but introduces the cloud-exclusive correctness authority that I9 and ADR 0010 §2 forbid. Rejected.
- **No scanner at all (gate-native only).** Defensible — the gate already does the SAST-equivalent work locally. Rejected only because free, non-gating breadth is worth having; it costs the gate nothing.
- **GitHub "default setup" instead of a workflow.** Zero-config, but the config (triggers, runner, languages) is not version-controlled or reviewable, and it defaults to running on every PR — reading as a gate. Rejected in favour of the advanced (workflow) setup.
- **macOS runner for CodeQL.** Would match the target but burns ~10× the CI minutes for zero benefit — static source analysis is platform-independent. Rejected.
- **Run on every PR.** More pre-merge visibility, but adds a per-PR check that reads as gating and costs minutes the gate already spends. Rejected in favour of push-to-main + schedule; can be added later as an explicitly non-required check.
