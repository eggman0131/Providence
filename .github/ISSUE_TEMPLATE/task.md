---
name: Task
about: A unit of work (feature, chore, refactor) aligned to the docs and contract
title: ""
labels: ["type:task"]
assignees: []
---

<!-- Add area:* and phase:N labels. If this turns out to need an architectural
     decision, add adr-needed and open/link an ADR rather than settling it here. -->

## What
<!-- The concrete change. One paragraph. -->

## Why
<!-- The force behind it. Link the governing doc/ADR if there is one. -->

## Definition of done
<!-- What "green" looks like for THIS issue. The gate (ADR 0009) is the sole
     definition of build-green; name here what must also be observed end-to-end
     (contract §3, Definition of Done). -->
- [ ] Gate green (`cargo gate`)
- [ ] Exercised end-to-end and observed, not just unit-tested

## Governance
<!-- Refs ADR-00NN for any decision that governs this. Commits/PRs: `Fixes #N`. -->
