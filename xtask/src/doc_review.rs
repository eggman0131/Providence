//! Doc-drift candidate detection (I6, ADR 0013).
//!
//! The **deterministic** half of the advisory doc-review workflow: given a git
//! commit range, decide *which* governance docs a change may have staled and
//! *why*. It does **not** judge whether drift is real — that is the local-LLM
//! advisor's job (`scripts/doc-review/run.sh`) — and it is **not** part of
//! `cargo gate`: it is advisory reporting, never a gate (the ADR 0011 stance).
//!
//! `cargo xtask doc-review [--since <ref>] [--json]` prints the candidates.
//! Three deterministic signals fire per doc:
//! 1. **Map** — a changed code/config path falls under an area the doc owns
//!    (the `doc-review.toml` table at the repo root).
//! 2. **Reference** — a changed ADR's number is cited in the doc's text.
//! 3. **I6 co-change** — code changed but the range updated no doc at all (a
//!    repository-level catch-all, only when nothing more specific fired).
//!
//! A doc that was itself edited in the range is never a candidate.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

/// A governed doc: its repo-relative path and its lower-cased text (for the
/// reference scan). Kept tiny so the core is testable without touching disk.
pub struct GovernedDoc {
    pub path: String,
    pub text_lower: String,
}

/// One `[[area]]` row of `doc-review.toml`: a path prefix and the docs that own
/// it. Editing the map is a tooling change, not an ADR-level change (cf. the
/// `magic.rs` / `boundary.rs` allow-lists).
pub struct Area {
    pub path: String,
    pub docs: Vec<String>,
}

/// The parsed `doc-review.toml` area table.
pub struct DocMap {
    pub areas: Vec<Area>,
}

/// A doc that may need review, with the human reasons and the changed paths
/// that triggered it.
pub struct Candidate {
    pub doc: String,
    pub reasons: Vec<String>,
    pub changed: Vec<String>,
}

/// Entry point for `cargo xtask doc-review`.
pub fn run(args: &[&str]) -> ExitCode {
    let mut since: Option<String> = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" => json = true,
            "--since" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("doc-review: --since needs a git ref");
                    return ExitCode::FAILURE;
                };
                since = Some((*value).to_string());
            }
            other => {
                eprintln!("doc-review: unknown argument `{other}`");
                eprintln!("usage: cargo xtask doc-review [--since <ref>] [--json]");
                return ExitCode::FAILURE;
            }
        }
        index += 1;
    }

    match detect(since.as_deref(), json) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("doc-review: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Resolve the range, read the inputs, compute candidates, print them.
fn detect(since: Option<&str>, json: bool) -> Result<ExitCode, String> {
    let root = crate::workspace_root();
    let range = resolve_range(since);
    let changed = git_changed_files(&range)?;

    let found = if changed.is_empty() {
        Vec::new()
    } else {
        let map = load_map(&root)?;
        let docs = load_governed_docs(&root)?;
        candidates(&changed, &docs, &map)
    };

    if json {
        let array: Vec<serde_json::Value> = found
            .iter()
            .map(|candidate| {
                serde_json::json!({
                    "doc": candidate.doc,
                    "reasons": candidate.reasons,
                    "changed": candidate.changed,
                })
            })
            .collect();
        let rendered = serde_json::to_string_pretty(&serde_json::Value::Array(array))
            .map_err(|error| format!("cannot render JSON: {error}"))?;
        println!("{rendered}");
    } else if found.is_empty() {
        println!("doc-review: no drift candidates in {range}");
    } else {
        println!("doc-review: {} drift candidate(s) in {range}:", found.len());
        for candidate in &found {
            println!("  • {}", candidate.doc);
            for reason in &candidate.reasons {
                println!("      - {reason}");
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// The pure core: decide which governed docs a change set may have staled.
///
/// Deterministic and disk-free so it can be unit-tested with synthetic inputs.
pub fn candidates(changed: &[String], docs: &[GovernedDoc], map: &DocMap) -> Vec<Candidate> {
    let changed_code: Vec<String> = changed
        .iter()
        .filter(|path| !is_doc_path(path))
        .cloned()
        .collect();
    let any_doc_changed = changed.iter().any(|path| is_doc_path(path));
    let adr_numbers = changed_adr_numbers(changed);

    let mut found = Vec::new();
    for doc in docs {
        // A doc updated in the same range is not stale by definition.
        if changed.iter().any(|path| path == &doc.path) {
            continue;
        }

        let mut reasons = Vec::new();
        let mut triggers = BTreeSet::new();

        // 1. Map signal.
        for area in &map.areas {
            if !area.docs.iter().any(|owned| owned == &doc.path) {
                continue;
            }
            let hits: Vec<&String> = changed
                .iter()
                .filter(|path| path.starts_with(&area.path))
                .collect();
            if !hits.is_empty() {
                reasons.push(format!("owns changed area `{}`", area.path));
                for hit in hits {
                    triggers.insert(hit.clone());
                }
            }
        }

        // 2. Reference signal (cited ADR whose file changed).
        for number in &adr_numbers {
            if doc.text_lower.contains(number.as_str()) {
                reasons.push(format!("cites ADR {number}, which changed"));
                for path in changed
                    .iter()
                    .filter(|path| path.contains(&format!("decisions/{number}")))
                {
                    triggers.insert(path.clone());
                }
            }
        }

        if !reasons.is_empty() {
            found.push(Candidate {
                doc: doc.path.clone(),
                reasons,
                changed: triggers.into_iter().collect(),
            });
        }
    }

    // 3. I6 co-change catch-all: behaviour changed but no doc was touched and
    //    nothing more specific fired.
    if found.is_empty() && !changed_code.is_empty() && !any_doc_changed {
        found.push(Candidate {
            doc: "(repository)".to_string(),
            reasons: vec![
                "code/config changed but no doc was updated in this range (I6)".to_string(),
            ],
            changed: changed_code,
        });
    }

    found.sort_by(|left, right| left.doc.cmp(&right.doc));
    found
}

/// Is this a governed-doc path (a `docs/**` markdown file or `CLAUDE.md`)?
fn is_doc_path(path: &str) -> bool {
    let is_markdown = Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
    path == "CLAUDE.md" || (path.starts_with("docs/") && is_markdown)
}

/// The four-digit numbers of any ADR files in the change set (e.g. `0009`).
fn changed_adr_numbers(changed: &[String]) -> Vec<String> {
    let mut numbers = Vec::new();
    for path in changed {
        if let Some(rest) = path.strip_prefix("docs/decisions/") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.len() == 4 {
                numbers.push(digits);
            }
        }
    }
    numbers.sort();
    numbers.dedup();
    numbers
}

/// The git range to diff: `<ref>..HEAD` when `--since` is given, else the range
/// about to be pushed (upstream → `origin/main` → previous commit).
fn resolve_range(since: Option<&str>) -> String {
    if let Some(reference) = since {
        return format!("{reference}..HEAD");
    }
    if let Ok(upstream) = git(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ]) {
        let upstream = upstream.trim();
        if !upstream.is_empty() {
            return format!("{upstream}..HEAD");
        }
    }
    if git(&["rev-parse", "--verify", "--quiet", "origin/main"]).is_ok() {
        return "origin/main..HEAD".to_string();
    }
    "HEAD~1..HEAD".to_string()
}

/// Files changed in `range`, repo-relative, forward-slashed.
fn git_changed_files(range: &str) -> Result<Vec<String>, String> {
    let output = git(&["diff", "--name-only", range])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect())
}

/// Run git from the workspace root, capturing stdout.
fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(crate::workspace_root())
        .output()
        .map_err(|error| format!("failed to launch git: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Parse the `[[area]]` table from `doc-review.toml`.
fn load_map(root: &Path) -> Result<DocMap, String> {
    let path = root.join("doc-review.toml");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|error| format!("doc-review.toml is not valid TOML: {error}"))?;

    let mut areas = Vec::new();
    if let Some(rows) = value.get("area").and_then(toml::Value::as_array) {
        for row in rows {
            let area_path = row
                .get("path")
                .and_then(toml::Value::as_str)
                .ok_or("doc-review.toml: an [[area]] is missing a string `path`")?
                .to_string();
            let docs = row
                .get("docs")
                .and_then(toml::Value::as_array)
                .ok_or_else(|| {
                    format!("doc-review.toml: area `{area_path}` is missing array `docs`")
                })?
                .iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect();
            areas.push(Area {
                path: area_path,
                docs,
            });
        }
    }
    Ok(DocMap { areas })
}

/// Enumerate the governed docs and read their text: `CLAUDE.md`, the top-level
/// `docs/*.md`, the ADRs under `docs/decisions/` (excluding the template), and
/// `docs/contracts/README.md`.
fn load_governed_docs(root: &Path) -> Result<Vec<GovernedDoc>, String> {
    let mut docs = Vec::new();
    read_governed(root, "CLAUDE.md", &mut docs)?;
    read_governed(root, "docs/contracts/README.md", &mut docs)?;
    collect_markdown(root, "docs", false, &mut docs)?;
    collect_markdown(root, "docs/decisions", true, &mut docs)?;
    Ok(docs)
}

/// Read every `*.md` directly under `dir` (skipping `template.md` in the ADR
/// directory) into the governed-doc list.
fn collect_markdown(
    root: &Path,
    dir: &str,
    skip_template: bool,
    docs: &mut Vec<GovernedDoc>,
) -> Result<(), String> {
    let absolute = root.join(dir);
    let entries = fs::read_dir(&absolute).map_err(|error| format!("cannot read {dir}: {error}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot list {dir}: {error}"))?
            .path();
        let is_markdown = path.extension().is_some_and(|extension| extension == "md");
        if !is_markdown {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if skip_template && name == "template.md" {
                continue;
            }
            names.push(name.to_string());
        }
    }
    names.sort();
    for name in names {
        read_governed(root, &format!("{dir}/{name}"), docs)?;
    }
    Ok(())
}

/// Read one governed doc (by repo-relative path) into the list.
fn read_governed(root: &Path, relative: &str, docs: &mut Vec<GovernedDoc>) -> Result<(), String> {
    let path = root.join(relative);
    if !path.exists() {
        return Ok(());
    }
    let text =
        fs::read_to_string(&path).map_err(|error| format!("cannot read {relative}: {error}"))?;
    docs.push(GovernedDoc {
        path: relative.to_string(),
        text_lower: text.to_lowercase(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str, text: &str) -> GovernedDoc {
        GovernedDoc {
            path: path.to_string(),
            text_lower: text.to_lowercase(),
        }
    }

    fn sample_map() -> DocMap {
        DocMap {
            areas: vec![
                Area {
                    path: "crates/config".to_string(),
                    docs: vec!["docs/40-parameterisation.md".to_string()],
                },
                Area {
                    path: "config/".to_string(),
                    docs: vec![
                        "docs/40-parameterisation.md".to_string(),
                        "docs/10-game-design.md".to_string(),
                    ],
                },
            ],
        }
    }

    #[test]
    fn map_signal_flags_the_owning_doc() {
        let changed = vec!["crates/config/src/lib.rs".to_string()];
        let docs = vec![
            doc("docs/40-parameterisation.md", "config things"),
            doc("docs/10-game-design.md", "gameplay"),
        ];
        let found = candidates(&changed, &docs, &sample_map());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].doc, "docs/40-parameterisation.md");
        assert_eq!(
            found[0].changed,
            vec!["crates/config/src/lib.rs".to_string()]
        );
    }

    #[test]
    fn a_doc_updated_in_range_is_suppressed() {
        let changed = vec![
            "crates/config/src/lib.rs".to_string(),
            "docs/40-parameterisation.md".to_string(),
        ];
        let docs = vec![doc("docs/40-parameterisation.md", "config")];
        assert!(candidates(&changed, &docs, &sample_map()).is_empty());
    }

    #[test]
    fn adr_reference_signal_flags_a_citing_doc() {
        let changed = vec!["docs/decisions/0009-enforcement-tooling-and-the-gate.md".to_string()];
        let docs = vec![
            doc(
                "docs/30-ai-agent-contract.md",
                "The gate is fixed by ADR 0009.",
            ),
            doc("docs/00-vision.md", "No decision reference here."),
        ];
        let found = candidates(&changed, &docs, &sample_map());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].doc, "docs/30-ai-agent-contract.md");
        assert!(
            found[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("0009"))
        );
    }

    #[test]
    fn i6_catchall_when_code_changed_and_no_docs_touched() {
        let changed = vec!["some/unmapped/module.rs".to_string()];
        let docs = vec![doc("docs/40-parameterisation.md", "config")];
        let found = candidates(&changed, &docs, &sample_map());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].doc, "(repository)");
    }

    #[test]
    fn i6_catchall_suppressed_when_a_specific_doc_fired() {
        let changed = vec!["crates/config/src/lib.rs".to_string()];
        let docs = vec![doc("docs/40-parameterisation.md", "config")];
        let found = candidates(&changed, &docs, &sample_map());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].doc, "docs/40-parameterisation.md");
    }

    #[test]
    fn no_changes_means_no_candidates() {
        let changed: Vec<String> = Vec::new();
        let docs = vec![doc("docs/40-parameterisation.md", "config")];
        assert!(candidates(&changed, &docs, &sample_map()).is_empty());
    }
}
