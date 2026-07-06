//! One-command environment provisioning (`cargo xtask setup`, I9).
//!
//! The Rust toolchain itself is pinned by `rust-toolchain.toml` (rustup
//! installs it automatically). This installs the pinned external cargo
//! subcommands and pre-fetches the advisory DB so subsequent gate runs work
//! offline. Network here is dev-time only (docs/60-constraints.md §2).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Pinned gate tools: (subcommand, crates.io name, version) — latest stable
/// at time of pinning (I8). Bumps arrive via reviewed PRs.
pub const PINNED_TOOLS: &[(&str, &str, &str)] = &[
    ("deny", "cargo-deny", "0.19.9"),
    ("llvm-cov", "cargo-llvm-cov", "0.8.7"),
];

/// Marker delimiters for the doc-review block in `.git/hooks/pre-push`, so the
/// install is idempotent and composes with any other pre-push hook content.
const HOOK_START: &str = "# providence-doc-review-hook-start";
const HOOK_END: &str = "# providence-doc-review-hook-end";

/// Install anything missing or version-drifted, then pre-fetch offline data.
pub fn setup() -> ExitCode {
    for (subcommand, crate_name, version) in PINNED_TOOLS {
        if is_installed_at(subcommand, version) {
            println!("setup: {crate_name} {version} already installed");
            continue;
        }
        println!("setup: installing {crate_name} {version} (pinned)…");
        let installed =
            crate::run::cargo(&["install", "--locked", crate_name, "--version", version]);
        if let Err(message) = installed {
            eprintln!("setup: failed to install {crate_name}: {message}");
            return ExitCode::FAILURE;
        }
    }

    // Pre-fetch the advisory DB so `cargo deny check` runs offline afterwards.
    println!("setup: fetching advisory database (dev-time network, I7-compatible)…");
    if let Err(message) = crate::run::cargo(&["deny", "fetch"]) {
        eprintln!("setup: advisory fetch failed: {message}");
        return ExitCode::FAILURE;
    }

    // Install the advisory doc-drift review pre-push hook (ADR 0013). Best
    // effort: a hook problem must never fail provisioning of the gate.
    match install_pre_push_hook() {
        Ok(()) => println!("setup: installed the doc-review pre-push hook (ADR 0013)"),
        Err(message) => {
            eprintln!("setup: could not install the doc-review hook (non-fatal): {message}");
        }
    }

    println!("setup: complete — run `cargo gate`");
    ExitCode::SUCCESS
}

/// Install (or refresh) the doc-review shim in `.git/hooks/pre-push`.
///
/// The shim just delegates to the versioned `scripts/hooks/pre-push`, so hook
/// logic stays under version control. Idempotent (marker-delimited) and
/// additive: existing pre-push content is preserved.
fn install_pre_push_hook() -> Result<(), String> {
    let hooks_dir = hooks_dir();
    fs::create_dir_all(&hooks_dir)
        .map_err(|error| format!("cannot create {}: {error}", hooks_dir.display()))?;
    let hook_path = hooks_dir.join("pre-push");
    let shim = pre_push_shim();

    let contents = if hook_path.exists() {
        let existing = fs::read_to_string(&hook_path)
            .map_err(|error| format!("cannot read {}: {error}", hook_path.display()))?;
        merge_block(&existing, &shim)
    } else {
        format!("#!/bin/sh\n{shim}\n")
    };
    fs::write(&hook_path, contents)
        .map_err(|error| format!("cannot write {}: {error}", hook_path.display()))?;
    make_executable(&hook_path)
}

/// The delegating shim written between the markers.
fn pre_push_shim() -> String {
    let body = r#"# Advisory doc-drift review on push (ADR 0013). Delegates to the versioned hook.
_pdr_root=$(git rev-parse --show-toplevel 2>/dev/null)
if [ -n "$_pdr_root" ] && [ -f "$_pdr_root/scripts/hooks/pre-push" ]; then
    sh "$_pdr_root/scripts/hooks/pre-push" "$@" || true
fi"#;
    format!("{HOOK_START}\n{body}\n{HOOK_END}")
}

/// Replace the existing marker block, or append a fresh one.
fn merge_block(existing: &str, shim: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(HOOK_START), existing.find(HOOK_END)) {
        let end = end + HOOK_END.len();
        format!("{}{}{}", &existing[..start], shim, &existing[end..])
    } else {
        let separator = if existing.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        format!("{existing}{separator}{shim}\n")
    }
}

/// The repo's git hooks directory (`git rev-parse --git-path hooks`, falling
/// back to `.git/hooks`).
fn hooks_dir() -> PathBuf {
    let root = crate::workspace_root();
    let resolved = Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(&root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|relative| !relative.is_empty());
    match resolved {
        Some(relative) => {
            let path = PathBuf::from(relative);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => root.join(".git").join("hooks"),
    }
}

/// Make a file user/group/other-executable (0o755). Unix-only; the sole
/// supported target is macOS (ADR 0005).
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("cannot chmod {}: {error}", path.display()))
}

/// Does `cargo <subcommand> --version` report exactly the pinned version?
fn is_installed_at(subcommand: &str, version: &str) -> bool {
    Command::new("cargo")
        .args([subcommand, "--version"])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains(version)
        })
}
