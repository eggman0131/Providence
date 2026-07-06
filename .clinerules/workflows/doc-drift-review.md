# Workflow — doc-drift-review

Advisory review of whether a code/config change has staled the project's
governance docs (I6, ADR 0013). You are the **advisor**: you judge drift and
propose issues, but you do **not** create issues, edit files, or touch the
network — a deterministic orchestrator (`scripts/doc-review/run.sh`) acts on the
JSON you emit.

## Inputs (from the invocation)

The prompt passes two values:

- `candidates=<path>` — a JSON file produced by `cargo xtask doc-review --json`.
  It is an array of `{ "doc", "reasons", "changed" }` objects. `doc` is a
  repo-relative doc path (or the literal `(repository)` for the I6 catch-all —
  "code changed but no doc was updated"). `changed` lists the paths that
  triggered the candidate.
- `range=<git-range>` — e.g. `origin/main..HEAD`; the commits under review.

If either value is missing, stop and emit an empty verdict array.

## Procedure

1. Read the candidates file. If it is empty (`[]`), emit an empty verdict array
   and stop.
2. For **each** candidate, gather evidence with read-only commands only:
   - Read the doc named by `doc` (skip file-reading when `doc` is
     `(repository)`; instead consider whether *any* doc should have been updated
     for the changed paths).
   - Read the changed files in `changed`, and inspect exactly what changed:
     `git diff <range> -- <each changed path>`.
3. Apply the verdict schema and criteria from the `docs are the constitution`
   rule. Decide `drift`, `side` (`doc` vs `code`), and `confidence`. Be
   conservative: when the doc and code do not actually contradict, `drift` is
   `false`. Prefer `side: "code"` only when the doc clearly mandates behaviour
   the change violates; otherwise `side: "doc"`.
4. Keep each `rationale` specific: name the doc lines and the changed paths that
   diverge. Write `title` as a short imperative suitable for a GitHub issue.

## Output (strict — the orchestrator parses this)

Your **final message** must contain **only** the verdict block — no prose, no
code fences, nothing before the opening marker or after the closing one. Do not
edit files or open issues. Print the verdict array as a single JSON block
wrapped **exactly** in these markers:

```
===DOC_DRIFT_JSON_BEGIN===
[
  {
    "doc": "docs/40-parameterisation.md",
    "drift": true,
    "side": "doc",
    "confidence": "high",
    "title": "Document the new sim.economy.upkeep key in 40-parameterisation",
    "rationale": "config/default.toml adds `sim.economy.upkeep` but §3 of docs/40-parameterisation.md lists no upkeep key.",
    "action": "Add `sim.economy.upkeep` to the parameter registry in docs/40-parameterisation.md §3."
  }
]
===DOC_DRIFT_JSON_END===
```

Emit one object per candidate you reviewed (include `drift: false` verdicts too —
the orchestrator filters them). If you reviewed nothing, emit `[]` between the
markers. Do not wrap the markers in code fences in your actual output.
