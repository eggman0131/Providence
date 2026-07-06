# Rule — docs are the constitution (I6)

This repository is governed by its docs, not the other way around. There is no
human code author; the `docs/` suite (and `CLAUDE.md`) *is* the contract.

- **Docs win over code (precedence, contract §1.2 / I6).** When code and a doc
  disagree, the doc is authoritative and the code is the defect — **unless** a
  newer accepted ADR in `docs/decisions/` supersedes the doc, in which case the
  ADR wins. Never silently "fix" a doc to match code without checking which is
  actually correct.
- **A stale doc is a defect (I6).** Behaviour changes must update the relevant
  doc in the same change; architectural changes add an ADR.

## Verdict schema (used by the `doc-drift-review` workflow)

When reviewing whether a change staled a doc, judge **conservatively** — only
report drift you can point to specifically — and, for each candidate reviewed,
emit exactly one JSON object with these fields:

```json
{
  "doc": "docs/40-parameterisation.md",
  "drift": true,
  "side": "doc",
  "confidence": "high",
  "title": "one-line issue title (imperative, <70 chars)",
  "rationale": "what specifically diverged — cite doc lines and changed paths",
  "action": "the concrete fix: what to change in the doc, or in the code"
}
```

- `drift`: `true` only if the doc and the changed code/config now genuinely
  disagree, or the doc omits behaviour it is supposed to describe. Cosmetic or
  unrelated changes are `false`.
- `side`: `"doc"` if the **doc** should change to match reality, `"code"` if the
  **code** violated what the doc mandates (the doc is right, the code is wrong),
  or `"none"` when `drift` is `false`.
- `confidence`: `"low" | "med" | "high"`. Use `"low"` when unsure — low-
  confidence findings are dropped by the orchestrator.
- Cite specifics (paths, line numbers, key names). Vague findings are noise.

Do not open issues, edit files, or run network commands yourself — this review
is **advisory**. The deterministic orchestrator acts on your verdict.
