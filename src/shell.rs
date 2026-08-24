//! POSIX shell resolution for spawning hook / tool commands.
//!
//! On unix the system `sh` is used. On Windows there is no `sh` on PATH by
//! default; Git for Windows ships one (`usr\bin\sh.exe`), so probe PATH and
//! the standard install locations before giving up. Callers degrade to a
//! no-op / tool failure when `None` is returned.

use std::ffi::OsString;
use std::path::PathBuf;

/// Locate a shell capable of running `-c <command>` (POSIX semantics).
pub fn resolve_shell() -> Option<OsString> {
    if let Ok(s) = std::env::var("NIKI_SHELL") {
        if !s.trim().is_empty() {
            return Some(OsString::from(s));
        }
    }

    if cfg!(windows) {
        const CANDIDATES: &[&str] = &[
            r"C:\Program Files\Git\usr\bin\sh.exe",
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\usr\bin\sh.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ];
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                for name in ["sh.exe", "bash.exe"] {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Some(candidate.into_os_string());
                    }
                }
            }
        }
        for candidate in CANDIDATES {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Some(path.into_os_string());
            }
        }
        None
    } else {
        Some(OsString::from("sh"))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn git_for_windows_ships_a_shell_so_this_resolves() {
        // NIKI requires git; on Windows that practically means Git for
        // Windows, which bundles sh.exe. If this fails the machine has an
        // exotic setup — set NIKI_SHELL explicitly.
        assert!(resolve_shell().is_some(), "no sh.exe/bash.exe found");
    }
}
