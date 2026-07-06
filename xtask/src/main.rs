//! The gate (contract §9, ADR 0009).
//!
//! `cargo gate` (alias for `cargo xtask gate`) runs every enforcement check
//! and is the single definition of "green" — locally and in CI (ADR 0010).
//! `cargo xtask setup` is the one-command environment provision (I9).
//! `cargo xtask schema --write` regenerates the committed schema artifact.

mod boundary;
mod coverage;
mod doc_review;
mod keys;
mod magic;
mod run;
mod schema;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match arg_refs.as_slice() {
        ["setup"] => tools::setup(),
        ["gate"] => gate(),
        ["schema"] => schema::check().map_or(ExitCode::FAILURE, |()| ExitCode::SUCCESS),
        ["schema", "--write"] => schema::write(),
        ["doc-review", rest @ ..] => doc_review::run(rest),
        _ => {
            eprintln!(
                "usage: cargo xtask <setup | gate | schema [--write] \
                 | doc-review [--since <ref>] [--json]>"
            );
            ExitCode::FAILURE
        }
    }
}

/// The workspace root (xtask's manifest dir is `<root>/xtask`).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask always lives one level below the workspace root")
        .to_path_buf()
}

/// One gate check: display name + the function that runs it.
type Check = (&'static str, fn() -> Result<(), String>);

/// Run every check in order, report all failures, exit non-zero on any red.
///
/// Order: cheap static checks first, the instrumented test+coverage run last.
fn gate() -> ExitCode {
    let checks: &[Check] = &[
        ("format (rustfmt --check)", check_format),
        ("lint (clippy -D warnings)", check_clippy),
        ("boundaries (crate-graph direction, I2/I4)", boundary::check),
        (
            "magic numbers (core behavioural literals, I1)",
            magic::check,
        ),
        (
            "schema drift (regenerate-and-diff, ADR 0008)",
            schema::check,
        ),
        (
            "config validity (layers + merged whole)",
            keys::check_config_validates,
        ),
        (
            "config keys (namespaces + schema integrity)",
            keys::check_keys,
        ),
        (
            "dependencies (cargo-deny: advisories/licenses/bans)",
            check_deny,
        ),
        (
            "tests + coverage (thresholds per crate, I5)",
            coverage::check,
        ),
    ];

    println!("gate: {} checks\n", checks.len());
    let mut failures = Vec::new();
    for (name, check) in checks {
        println!("──► {name}");
        match check() {
            Ok(()) => println!("  ✓ {name}\n"),
            Err(message) => {
                println!("  ✗ {name}\n    {message}\n");
                failures.push((*name, message));
            }
        }
    }

    if failures.is_empty() {
        println!("gate: GREEN ({} checks)", checks.len());
        ExitCode::SUCCESS
    } else {
        println!(
            "gate: RED — {} of {} checks failed:",
            failures.len(),
            checks.len()
        );
        for (name, _) in &failures {
            println!("  ✗ {name}");
        }
        ExitCode::FAILURE
    }
}

/// Canonical style: rustfmt defaults, checked, zero tolerance (§6.2).
fn check_format() -> Result<(), String> {
    run::cargo(&["fmt", "--all", "--", "--check"])
}

/// Type check + lints: clippy over everything, all warnings denied (§6.2).
fn check_clippy() -> Result<(), String> {
    run::cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])
}

/// External-dependency policy: advisories, licenses, bans, sources (I8).
fn check_deny() -> Result<(), String> {
    run::cargo(&["deny", "check"])
}
