# 0010 — Branch-based workflow with PRs and CI that runs the gate

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§1, §5, §6, §9; I8, I9), [`../60-constraints.md`](../60-constraints.md) (§5), [`0005`](./0005-macbook-only-offline-runtime.md), [`0009`](./0009-enforcement-tooling-and-the-gate.md)

## Context

The repository is now on GitHub. We need a durable contribution workflow that serves not only the human director and local agent sessions but also **contributors that never run the gate locally** — Dependabot dependency bumps and cloud/remote AI agents.

Invariant **I9** says "what runs locally is exactly what 'CI' would run — there is no separate cloud CI to hide behind," and [`60-constraints.md`](../60-constraints.md) §5 / [ADR 0005](./0005-macbook-only-offline-runtime.md) say "no cloud CI is assumed." These forbid **cloud-*exclusive* checks** — correctness that can't be reproduced on the MacBook — **not CI itself**. For a change authored by a bot or a cloud agent there is no local gate run, so CI is the natural (and desirable) venue in which the gate executes before merge. There is **no human code author** (§1), which determines what merge "gating" should mean.

## Decision

1. **Branch-and-PR.** After the initial bootstrap commit, there are **no direct pushes to `main`**. Every change lands via a short-lived branch → pull request → **squash-merge**. Branch names: `adr-NNNN-slug` (decisions), `phase-N-slug` (bootstrapping phases), `feat/…` `fix/…` `docs/…` otherwise. Branches are deleted on merge; `main` is never force-pushed.

2. **The gate is the single definition of correctness, executable locally *or* in CI.** A change is mergeable **iff `cargo gate` passes on it, wherever it ran** ([ADR 0009](./0009-enforcement-tooling-and-the-gate.md)). CI runs on **Apple-Silicon macOS runners** (`macos-14`+, `aarch64-apple-darwin`) to match the only supported target ([ADR 0005](./0005-macbook-only-offline-runtime.md), [ADR 0006](./0006-rust-language-and-runtime.md)), and runs **only** the canonical gate — **no bespoke CI-only checks**. Therefore CI-green ⟺ local-gate-green by construction, and I9 stays literally true.

3. **CI is a *required* merge check.** Branch protection on `main` requires the `cargo gate` status check to pass. It does **not** require a human approving review — there is no human code author (§1); correctness is enforced by the gate, not by sign-off. The director may still review and merges at will.

4. **Non-local and automated contributors are first-class.** Dependabot (dependency freshness/anti-rot, I8) and cloud/remote agents open PRs gated by CI exactly like local work. This is the safety net that "no cloud CI is assumed" is often misread to forbid; it is explicitly endorsed here. The gate's `cargo-deny` + full test/boundary/determinism suite is precisely the right check for a dependency bump.

5. **ADR lifecycle via PR.** New ADRs are `Proposed` in their PR and become `Accepted` on merge. (This bootstrapping ADR is accepted as decided by the director in-session, consistent with 0001–0009.)

6. **Sequencing.** This ADR sets the policy now. The functional `.github/workflows/ci.yml` (runs `cargo gate`), `.github/dependabot.yml`, and the required-status-check branch protection land with the **Phase-1 gate**, because they need `cargo gate` and a `Cargo.toml` to exist. Until then, `main` may use basic protection (PR required; no force-push or deletion).

7. **Offline integrity (I7).** CI provisioning fetches the pinned toolchain and cargo subcommands at build time — dev-time network, permitted by [`60-constraints.md`](../60-constraints.md) §2. The gate and the game remain fully offline; **CI is never a runtime dependency** of the game.

## Consequences

- **Positive:** every contribution — human, local agent, Dependabot, cloud agent — passes the identical gate before merge; a clean arm64 macOS runner that matches the target catches "never ran locally" drift, strengthening I9 reproducibility; linear, reviewable history and a durable record; the local gate stays complete and authoritative (no cloud-exclusive checks).
- **Negative / trade-offs:** PR overhead per change; macOS arm64 CI minutes cost more than Linux (accepted — the target demands it); a public repo means CI logs are public; a bootstrapping wrinkle (the required check can't exist until the gate does, so it lands with Phase-1).
- **Enforcement / gate impact:** adds a CI workflow that runs the *existing* gate plus a required-status-check on `main`, and a Dependabot config — **no new gate checks**. Clarifies the CI relationship in I9 / ADR 0005 / `60-constraints.md` §5.
- **Docs to update (this change):** `decisions/README.md` (index); `60-constraints.md` §5 (reworded to "no cloud CI is *required*"); `0005` (refinement note). I9's wording is kept as-is — it is already compatible.

## Alternatives considered

- **Commit straight to `main`.** Simplest, but no reviewable units, no CI gate, and no safe path for Dependabot/cloud-agent contributions. Rejected.
- **CI as an independent authority / cloud-exclusive checks.** Would create the "separate cloud CI to hide behind" that I9 forbids. Rejected — CI runs only `cargo gate`.
- **Linux CI runners (cheaper).** Wrong target; determinism and build must match `aarch64-apple-darwin`. Rejected.
- **Require a human approving review to merge.** Contradicts the no-human-author model (§1); correctness is the gate's job. Rejected — review is optional, the gate is required.
- **Optional (non-required) CI.** Would not reliably gate the bot/cloud-agent PRs that most motivate CI. Rejected in favour of a required check.
