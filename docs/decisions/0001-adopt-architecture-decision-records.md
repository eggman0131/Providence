# 0001 — Adopt Architecture Decision Records

- **Status:** Accepted
- **Date:** 2026-07-02
- **Deciders:** Founding session (human director + agent)
- **Related:** [`../30-ai-agent-contract.md`](../30-ai-agent-contract.md) §5, §8

## Context

The codebase has **no human author**; it is built and maintained by AI agents across many independent sessions. Significant decisions made in one session are invisible to the next unless they are written down in a durable, discoverable way. Without a record, agents will re-litigate settled choices, silently contradict each other, and let the architecture drift.

## Decision

We will record every significant/architectural decision as a numbered **ADR** in [`docs/decisions/`](./), following the process in [`README.md`](./README.md). ADRs are the **only** sanctioned mechanism for architectural change and for amending the AI-agent contract. Balance/content/number changes are exempt (they are config tasks).

## Consequences

- **Positive:** durable, linkable rationale; cross-session continuity; a clear amendment path for the contract; decisions are auditable.
- **Negative / trade-offs:** a small ceremony cost per architectural change.
- **Enforcement / gate impact:** the contract's change-classification (§5) makes an ADR part of "done" for architectural changes; a lightweight check may later verify that architectural changes touch `decisions/`.
- **Docs to update:** contract §5/§8 reference this process; `decisions/README.md` holds the index.

## Alternatives considered

- **Commit messages / wiki only:** not discoverable or structured enough for agents loading limited context.
- **No formal record:** guarantees drift in an author-less codebase. Rejected.
