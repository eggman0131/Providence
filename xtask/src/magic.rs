//! Magic-number scan (I1, docs/40-parameterisation.md §6.2, ADR 0009 §3).
//!
//! Scans the deterministic core's sources for behavioural numeric literals
//! outside the allow-list (`0`, `1`). A flagged literal either becomes a
//! config parameter or — if genuinely structural, e.g. an algorithm
//! constant — carries the explicit, grep-able annotation
//! `gate:allow(magic) <reason>` on its line or the line above.
//!
//! Scope (Phase 1): integer and float literals in non-test code of
//! `crates/core`. String literals are not yet scanned; tightening the scan
//! is a normal reviewed change.

use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// The explicit escape-hatch annotation.
const ALLOW_MARKER: &str = "gate:allow(magic)";

/// Directories whose non-test sources are scanned.
const SCANNED_ROOTS: &[&str] = &["crates/core/src"];

/// Scan the core for unannotated behavioural literals.
pub fn check() -> Result<(), String> {
    let root = crate::workspace_root();
    let mut violations = Vec::new();
    for scanned in SCANNED_ROOTS {
        let dir = root.join(scanned);
        let mut files = Vec::new();
        collect_rust_files(&dir, &mut files)
            .map_err(|error| format!("cannot walk {}: {error}", dir.display()))?;
        for file in files {
            scan_file(&file, &root, &mut violations)?;
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "behavioural literals in core code (move to config per I1, or annotate \
             `{ALLOW_MARKER} <reason>` if structural):\n    {}",
            violations.join("\n    ")
        ))
    }
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn scan_file(file: &Path, root: &Path, violations: &mut Vec<String>) -> Result<(), String> {
    let source = fs::read_to_string(file)
        .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    let parsed = syn::parse_file(&source)
        .map_err(|error| format!("cannot parse {}: {error}", file.display()))?;
    let lines: Vec<&str> = source.lines().collect();
    let display_path = file
        .strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string();

    let mut scan = LiteralScan {
        lines: &lines,
        display_path: &display_path,
        violations,
    };
    scan.visit_file(&parsed);
    Ok(())
}

struct LiteralScan<'src> {
    lines: &'src [&'src str],
    display_path: &'src str,
    violations: &'src mut Vec<String>,
}

impl LiteralScan<'_> {
    /// Is the literal's line (or the line above) annotated with the marker?
    fn is_annotated(&self, line_number: usize) -> bool {
        // Span lines are 1-based.
        let same_line = self.lines.get(line_number.saturating_sub(1));
        let line_above = if line_number >= 2 {
            self.lines.get(line_number - 2)
        } else {
            None
        };
        same_line.is_some_and(|line| line.contains(ALLOW_MARKER))
            || line_above.is_some_and(|line| line.contains(ALLOW_MARKER))
    }

    fn flag(&mut self, line_number: usize, literal_text: &str) {
        if !self.is_annotated(line_number) {
            self.violations.push(format!(
                "{}:{line_number}: literal `{literal_text}`",
                self.display_path
            ));
        }
    }
}

impl<'ast> Visit<'ast> for LiteralScan<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if has_cfg_test(item) {
            return; // test code is exempt (docs/40-parameterisation.md §6.2)
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_lit_int(&mut self, literal: &'ast syn::LitInt) {
        let allowed = literal.base10_parse::<u128>().is_ok_and(|value| value <= 1);
        if !allowed {
            self.flag(literal.span().start().line, &literal.to_string());
        }
    }

    fn visit_lit_float(&mut self, literal: &'ast syn::LitFloat) {
        self.flag(literal.span().start().line, &literal.to_string());
    }
}

/// Does this item carry `#[cfg(test)]`?
fn has_cfg_test(item: &syn::Item) -> bool {
    let attrs = match item {
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        _ => return false,
    };
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .meta
                .require_list()
                .is_ok_and(|list| list.tokens.to_string().contains("test"))
    })
}
