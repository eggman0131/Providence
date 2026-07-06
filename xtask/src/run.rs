//! Small helper for running external commands from the workspace root.

use std::process::Command;

/// Run `cargo <args>` from the workspace root, streaming output; error on a
/// non-zero exit.
pub fn cargo(args: &[&str]) -> Result<(), String> {
    command("cargo", args)
}

/// Run an arbitrary command from the workspace root, streaming output.
pub fn command(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(crate::workspace_root())
        .status()
        .map_err(|error| format!("failed to launch `{program}`: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{program} {}` exited with {status}",
            args.join(" ")
        ))
    }
}
