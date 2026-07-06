# 0012 — Project management and issue tracking on GitHub

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Human director + agent session
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) (§1, §4, §5, §8; I6, I7), [`0001`](./0001-adopt-architecture-decision-records.md), [`0009`](./0009-enforcement-tooling-and-the-gate.md), [`0010`](./0010-branch-workflow-and-ci.md), [`0011`](./0011-advisory-code-scanning.md)

## Context

The repository already lives on GitHub: code, pull requests, CI running the gate ([ADR 0010](./0010-branch-workflow-and-ci.md)), Dependabot, and advisory scanning ([ADR 0011](./0011-advisory-code-scanning.md)). What has **no single home** is *work tracking* — tasks, bugs, and the roadmap/"what's next." It currently lives informally in "Status" prose across the docs and the "Open (not yet decided)" note at the foot of [`decisions/README.md`](./README.md). The director wants **one place and one structure**, explicitly to reduce cognitive overhead (forgetting where things are tracked is a real force, not a hypothetical).

Two constraints shape the choice:

1. **Decisions already have a home and must keep it.** Architectural/process decisions are ADRs ([ADR 0001](./0001-adopt-architecture-decision-records.md)): versioned with the code, reviewed by PR, offline-readable, agent-readable, and precedence-ranked (contract §1.2). Work tracking must not dilute or duplicate that record.
2. **This is a *development-process* choice, not a runtime one.** I7 (offline) governs the game runtime; development already depends on GitHub cloud for CI ([ADR 0010](./0010-branch-workflow-and-ci.md) §7). Hosting project management on GitHub adds no new *runtime* dependency.

There is no human code author (§1), so the chosen surface must also be reachable and writable by agents (the `gh` CLI and the GitHub MCP), not just a human UI.

## Decision

1. **GitHub is the single home for work tracking.** Tasks and bugs are **GitHub Issues**; the roadmap and "what's next" is one **GitHub Project (v2)** board. We do not adopt a second project-management tool.

2. **Decisions stay as ADRs; a clean seam between the two.** ADRs remain the authoritative record of *why* (architecture, process, invariants). Issues track *what* and *when*. An issue references the ADR that governs it; an issue that turns out to require an architectural decision spawns an ADR (label `adr-needed`) rather than being settled in the issue thread. The contract's precedence order (§1.2) is unchanged: ADRs and docs outrank issue/PR discussion.

3. **A small, doc-aligned structure.** Issue templates (`task`, `bug`, `adr-needed`); a label taxonomy mirroring the codebase — `area:core|config|ports|llm-opponent|tooling|docs`, `phase:2`…`phase:6` matching the bootstrapping order (contract §7.4), and `type:*`; one Project with a **"What's next"** view. The scattered "Status/Open" notes in the docs point at the Project as the single source of truth for planning, and the existing open items (LLM runtime & model; debug/HUD UI) migrate to issues.

4. **Frictionless, self-linking capture.** One-line intake (`gh issue create`) is the default so nothing is lost between sessions. Commits and PRs reference or close issues (`Fixes #N`, `Refs #N`) and cite governing ADRs (`Refs ADR-00NN`), so the work↔code↔decision links close themselves.

5. **The roadmap is advisory, not authority.** The Project board reflects intent and ordering; it does **not** gate merges and is **not** part of the Definition of Done. The gate ([ADR 0009](./0009-enforcement-tooling-and-the-gate.md)) remains the sole definition of "green" — consistent with keeping mutable, non-deterministic surfaces out of the gate.

6. **Scope boundary.** This ADR adds nothing to the gate, does not alter the branch/PR/CI workflow ([ADR 0010](./0010-branch-workflow-and-ci.md)), and does not make GitHub a runtime dependency (I7 intact).

## Consequences

- **Positive:** one login and URL for code, review, CI status, backlog, and roadmap — directly serving the low-overhead/forgetfulness force; the surface is agent-accessible via `gh`/MCP; capture is one command and self-linking; *decisions* and *work* stay cleanly separated, each in the form that suits it; no new gate surface to build or maintain.
- **Negative / trade-offs:** GitHub Issues/Projects is lighter than dedicated tools (no sprints, estimates, or cycle automation) — accepted for a solo, agent-driven project where less ceremony is a feature; work-tracking data lives in GitHub's cloud (exportable via the API — no lock-in worth mitigating now); on a public repo, issues and roadmap are public (acceptable, and consistent with public CI logs per [ADR 0010](./0010-branch-workflow-and-ci.md)).
- **Enforcement / gate impact:** **none.** No new gate checks; the gate and Definition of Done are unchanged. Issue templates and labels are declarative config under `.github/`, reviewed like any other change.
- **Docs to update:** on acceptance — `decisions/README.md` (add the 0012 index row; replace the "Open (not yet decided)" note with a pointer to the Project/issues); a short "Project management" note where planning currently lives (e.g. [`../README.md`](../README.md) Status, [`README.md`](./README.md)) naming the Project as the single source of truth for "what's next"; optionally a one-line pointer in `CLAUDE.md` telling agents where the backlog lives. The index row lands now (as `Proposed`); the remaining edits and the migration of open items land with the scaffold, when the Project and issues exist (sequencing mirrors [ADR 0010](./0010-branch-workflow-and-ci.md) §6).

## Alternatives considered

- **A dedicated PM tool (Linear / Jira / Notion).** Stronger boards and automation, but a *second* home to check and keep in sync — the exact fragmentation to avoid — plus an extra auth/connector surface for agents (the Linear MCP is not even authorized in the current toolchain). Rejected.
- **An in-repo markdown backlog (`BACKLOG.md` / tasks in docs).** Fully offline, agent-owned, versioned. Rejected as the *primary* home: no board, notifications, or native cross-linking to PRs, and it re-implements issue tracking GitHub already provides. It remains a viable offline fallback, and the durable/offline-critical layer is already covered by ADRs.
- **Track decisions as GitHub Issues too (fold ADRs into issues).** Rejected: ADRs are versioned, offline, PR-reviewed, and precedence-ranked (§1.2, [ADR 0001](./0001-adopt-architecture-decision-records.md)); issues are mutable and cloud-only. Conflating them weakens the governance record.
- **Status quo (informal notes in docs).** Rejected: no single source of truth and easy to lose items — the problem this ADR exists to solve.
