//! `niki ui` — launch the NIKI terminal interface.
//!
//! The interface is a TypeScript/Ink program, so it cannot live inside the Rust binary. It ships
//! beside it as a self-contained executable produced by `bun build --compile`, and this command
//! is how a user reaches it.
//!
//! Without this, installing NIKI gave you the engine and no interface at all: `niki chat` was the
//! old ratatui TUI this project exists to replace. That is the gap this closes.
//!
//! Resolution order, first hit wins:
//!   1. `--shell <path>`, for a developer pointing at their own build
//!   2. `$NIKI_SHELL_BIN`
//!   3. `niki-shell` next to this executable — how a release archive and a Homebrew install lay it out
//!   4. `shell/dist/niki-shell` in a source checkout, for working on the interface itself
//!
//! If none of those exist the command says exactly which one to build, rather than printing an
//! empty screen and leaving the user to guess.

use std::path::PathBuf;
use std::process::Command;

use clap::Args;

#[derive(Args, Debug, Default)]
pub struct UiArgs {
    /// Explicit path to the interface executable. Overrides every other lookup.
    #[arg(long)]
    pub shell: Option<PathBuf>,

    /// Arguments passed through to the interface, after `--`.
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,
}

const BUILD_HINT: &str = "\
The NIKI interface ships as a separate self-contained executable next to `niki`.
It is not in your PATH, and no bundled copy was found.

To build it from a source checkout:
    cd shell && bun install && npm run build:binary

Then run `niki ui` again.";

/// Every place a bundled interface could plausibly be, in priority order.
pub fn candidates(
    explicit: Option<&PathBuf>,
    exe: Option<&PathBuf>,
    cwd: &std::path::Path,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(path) = explicit {
        out.push(path.clone());
    }
    if let Ok(from_env) = std::env::var("NIKI_SHELL_BIN") {
        if !from_env.is_empty() {
            out.push(PathBuf::from(from_env));
        }
    }
    if let Some(exe) = exe.and_then(|p| p.parent()) {
        for name in ["niki-shell", "niki-shell.exe"] {
            out.push(exe.join(name));
        }
    }
    out.push(cwd.join("shell").join("dist").join("niki-shell"));
    if let Some(exe) = exe.and_then(|p| p.parent()) {
        out.push(exe.join("shell").join("dist").join("niki-shell"));
    }
    out
}

/// The first candidate that exists and is executable.
pub fn locate(args: &UiArgs) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    candidates(args.shell.as_ref(), exe.as_ref(), cwd.as_path())
        .into_iter()
        .find(|p| p.is_file())
}

pub fn handle(args: &UiArgs) -> Result<(), crate::NikiError> {
    let Some(shell) = locate(args) else {
        return Err(crate::NikiError::Config(format!(
            "could not find the NIKI interface executable.\n\n{BUILD_HINT}"
        )));
    };

    // The engine is this executable. Handing the interface the absolute path means it does not
    // have to be on PATH, which it usually is not in a shell that just installed NIKI.
    let engine = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("niki"));
    let status = Command::new(&shell)
        .arg("--engine")
        .arg(&engine)
        .args(&args.args)
        .status()
        .map_err(|e| {
            crate::NikiError::Config(format!(
                "could not start the interface at {}: {e}",
                shell.display()
            ))
        })?;

    // The interface restores its own terminal; a non-zero exit here is the interface's business,
    // not a failure to launch one, so it is reported rather than turned into an error.
    if !status.success() {
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_path_wins_over_everything() {
        let found = candidates(
            Some(&PathBuf::from("/explicit/niki-shell")),
            Some(&PathBuf::from("/usr/local/bin/niki")),
            std::path::Path::new("/work"),
        );
        assert_eq!(
            found.first().map(|p| p.as_path()),
            Some(std::path::Path::new("/explicit/niki-shell"))
        );
    }

    #[test]
    fn the_release_layout_is_looked_for_next_to_the_binary() {
        let found = candidates(
            None,
            Some(&PathBuf::from("/usr/local/bin/niki")),
            std::path::Path::new("/work"),
        );
        // A Homebrew install puts both in the same prefix; the interface must be found there.
        assert!(
            found.contains(&PathBuf::from("/usr/local/bin/niki-shell")),
            "expected the sibling path, got {found:?}"
        );
    }

    #[test]
    fn a_source_checkout_is_the_last_resort() {
        let found = candidates(
            None,
            Some(&PathBuf::from("/opt/niki/bin/niki")),
            std::path::Path::new("/work"),
        );
        assert!(found.contains(&PathBuf::from("/work/shell/dist/niki-shell")));
    }
}
