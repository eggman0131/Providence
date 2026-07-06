#!/usr/bin/env python3
"""Parse the doc-drift verdict and open deduped GitHub issues (ADR 0013).

Reads (via environment, set by run.sh):
  REVIEW_OUT path to the agent's captured stdout (contains the marker-wrapped JSON)
  CANDFILE   path to the `cargo xtask doc-review --json` candidates
  RANGE      the git range under review, for the issue body
  DRY_RUN    "1" to print would-be issues instead of creating them
  PDR_AGENT  which advisor produced REVIEW_OUT (cline | claude), for escalation

This is the *actuator* half of the advisor/actuator split (cf. ADR 0002): the
agent only advises; issue creation, dedup, and labelling are deterministic and
done here. When the agent returns no usable verdict the run is **not** treated
as a clean pass — it prints a loud escalation pointing at an online agent, so a
missed review is never mistaken for "no drift". It never raises to the caller —
a doc review must never break a push.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

MARKERS = re.compile(
    r"===DOC_DRIFT_JSON_BEGIN===(.*?)===DOC_DRIFT_JSON_END===", re.DOTALL
)
ANSI = re.compile(r"\x1b\[[0-9;]*m")
FINGERPRINT = re.compile(r"doc-drift:fp=([0-9a-f]+)")
CONFIDENCE = {"low": 0, "med": 1, "medium": 1, "high": 2}
MIN_CONFIDENCE = 1  # med or higher

# Docs whose drift is architectural — flag for an ADR (CLAUDE.md, ADR 0012).
ARCHITECTURAL_DOCS = {"docs/20-architecture.md", "docs/30-ai-agent-contract.md"}


def read_verdicts(path):
    """Extract and parse the marker-wrapped verdict array from Cline output."""
    try:
        raw = open(path, encoding="utf-8", errors="replace").read()
    except OSError as error:
        print(f"[doc-review] cannot read Cline output: {error}")
        return None
    raw = ANSI.sub("", raw)
    match = MARKERS.search(raw)
    payload = match.group(1).strip() if match else last_json_array(raw)
    if payload is None:
        print("[doc-review] no verdict block or recoverable JSON in the agent output")
        return None
    try:
        verdicts = json.loads(payload)
    except json.JSONDecodeError as error:
        print(f"[doc-review] could not parse verdict JSON: {error}")
        return None
    if not isinstance(verdicts, list):
        print("[doc-review] verdict block was not a JSON array — ignoring")
        return None
    if not match:
        print("[doc-review] recovered verdict via fallback (markers were missing)")
    return verdicts


def last_json_array(text):
    """Best-effort fallback: the last balanced top-level ``[...]`` in the text.

    Used only when the model omitted the markers. Returns the raw slice or None.
    """
    end = text.rfind("]")
    while end != -1:
        depth = 0
        for index in range(end, -1, -1):
            char = text[index]
            if char == "]":
                depth += 1
            elif char == "[":
                depth -= 1
                if depth == 0:
                    candidate = text[index : end + 1]
                    if '"drift"' in candidate or candidate.strip() == "[]":
                        return candidate
                    break
        end = text.rfind("]", 0, end)
    return None


def changed_by_doc(cand_path):
    """Map each candidate doc to the changed paths that triggered it."""
    mapping = {}
    try:
        for candidate in json.load(open(cand_path, encoding="utf-8")):
            mapping[candidate.get("doc", "")] = candidate.get("changed", [])
    except (OSError, json.JSONDecodeError):
        pass
    return mapping


def escalate(agent, docs, git_range):
    """Loud, actionable message when the agent returned no usable verdict.

    An inconclusive review is a *missed* review, not a clean pass — surface it
    and point at the more reliable online agent (Claude Haiku).
    """
    bar = "=" * 68
    print(bar)
    print(f"[doc-review] ⚠ REVIEW INCONCLUSIVE — the '{agent}' agent returned no usable verdict.")
    print(f"[doc-review]   {len(docs)} candidate doc(s) for {git_range} went UNREVIEWED")
    print("[doc-review]   (a missed review, NOT a clean 'no drift' pass):")
    for doc in docs:
        print(f"[doc-review]     - {doc}")
    if agent != "claude":
        print("[doc-review]   → Re-run with the online agent (Claude Haiku — reliable at the JSON contract):")
        print(f"[doc-review]       scripts/doc-review/run.sh --agent claude --range {git_range}")
    else:
        print("[doc-review]   → The online agent also failed — review these docs against the range by hand,")
        print("[doc-review]     or inspect the captured agent output for a partial verdict.")
    print(bar)


def candidate_docs(cand_path):
    """The list of candidate doc names (for the escalation message)."""
    try:
        return [c.get("doc", "?") for c in json.load(open(cand_path, encoding="utf-8"))]
    except (OSError, json.JSONDecodeError):
        return []


def existing_fingerprints():
    """Fingerprints already tracked by an open `doc-drift` issue (dedup).

    Scans issue bodies locally rather than trusting GitHub's search index, so
    dedup is deterministic.
    """
    try:
        result = subprocess.run(
            ["gh", "issue", "list", "--state", "open", "--label", "doc-drift",
             "--json", "number,body", "--limit", "200"],
            capture_output=True, text=True, check=False,
        )
    except OSError as error:
        print(f"[doc-review] cannot list issues (dedup disabled): {error}")
        return set()
    if result.returncode != 0:
        print(f"[doc-review] gh issue list failed (dedup disabled): {result.stderr.strip()}")
        return set()
    found = set()
    for issue in json.loads(result.stdout or "[]"):
        found.update(FINGERPRINT.findall(issue.get("body", "")))
    return found


def side_phrase(side):
    if side == "doc":
        return "update the **doc** to match the code"
    if side == "code":
        return "the **code** violates what the doc mandates — fix the code"
    return "n/a"


REVIEWERS = {"cline": "local Qwen (via Cline)", "claude": "online Claude Haiku"}


def build_body(verdict, doc, side, fingerprint, changed, git_range, agent):
    changed_md = "\n".join(f"- `{path}`" for path in changed) or "- (see the range diff)"
    reviewer = REVIEWERS.get(agent, agent)
    return f"""_Advisory finding from the doc-drift review (ADR 0013), reviewed by {reviewer}. Not a gate — close if wrong._

- **Doc:** `{doc}`
- **Fix side:** {side_phrase(side)}
- **Confidence:** {verdict.get('confidence', '?')}
- **Range:** `{git_range}`

**Why it may be drifting**
{verdict.get('rationale', '(none given)')}

**Suggested action**
{verdict.get('action', '(none given)')}

**Triggering changes**
{changed_md}

<!-- doc-drift:fp={fingerprint} -->
"""


def main():
    dry_run = os.environ.get("DRY_RUN") == "1"
    git_range = os.environ.get("RANGE", "")
    agent = os.environ.get("PDR_AGENT", "cline")
    cand_path = os.environ["CANDFILE"]
    verdicts = read_verdicts(os.environ["REVIEW_OUT"])
    if verdicts is None:
        # No usable verdict — a missed review, not a clean pass. Escalate loudly.
        escalate(agent, candidate_docs(cand_path), git_range)
        return
    if not verdicts:
        print("[doc-review] agent reviewed the candidates and found no drift — nothing to post")
        return
    triggers = changed_by_doc(cand_path)
    existing = existing_fingerprints()

    posted = skipped = 0
    for verdict in verdicts:
        if not isinstance(verdict, dict) or not verdict.get("drift"):
            continue
        if CONFIDENCE.get(str(verdict.get("confidence", "low")).lower(), 0) < MIN_CONFIDENCE:
            skipped += 1
            continue
        doc = verdict.get("doc", "(repository)")
        side = verdict.get("side", "doc")
        # Fingerprint on the doc only (not doc+side): the model's doc-vs-code
        # "side" call varies run to run, so keying on side would file duplicate
        # issues for the same drift. One open issue per drifted doc; the body
        # carries the side.
        fingerprint = hashlib.sha1(doc.encode()).hexdigest()[:12]
        if fingerprint in existing:
            skipped += 1
            print(f"[doc-review] dedup: an open issue already tracks {doc} ({side}) [{fingerprint}]")
            continue

        title = (verdict.get("title") or f"Doc drift: {doc}").strip()[:120]
        labels = ["doc-drift", "area:docs", "documentation"]
        if side == "doc" and (doc.startswith("docs/decisions/") or doc in ARCHITECTURAL_DOCS):
            labels.append("adr-needed")
        body = build_body(verdict, doc, side, fingerprint, triggers.get(doc, []), git_range, agent)

        if dry_run:
            print(f"\n[doc-review] [dry-run] would open issue (labels: {', '.join(labels)})")
            print(f"  title: {title}")
            for line in body.splitlines():
                print(f"    {line}")
            posted += 1
            continue

        command = ["gh", "issue", "create", "--title", title, "--body", body]
        for label in labels:
            command += ["--label", label]
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        if result.returncode == 0:
            print(f"[doc-review] opened: {result.stdout.strip()}")
            posted += 1
        else:
            print(f"[doc-review] gh issue create failed for {doc}: {result.stderr.strip()}")

    verb = "previewed" if dry_run else "opened"
    print(f"[doc-review] done — {posted} issue(s) {verb}, {skipped} skipped")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:  # never break the push
        print(f"[doc-review] unexpected error (ignored): {error}")
        sys.exit(0)
