//! Secure file-writing helpers.
//!
//! Artifact files (reports, patches, audit logs, session/goal state) may contain
//! pipeline output or tool arguments that could include secrets. Writing them with
//! user-only permissions (0600) limits exposure on multi-user hosts. See research
//! report S11.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Write `contents` to `path` with user-only read/write permissions (0600).
///
/// **Atomic.** The write goes to a temporary file in the same directory and is
/// then renamed over the target, so a crash or a full disk mid-write leaves the
/// *previous* contents intact rather than a truncated file.
///
/// This used to be a bare `std::fs::write`, and fifteen call sites relied on
/// that: `checkpoint.json`, the goal state, `report.md`, `changes.patch`, every
/// `artifacts/*.json`, and `SALVAGED.md` — the last of which is written
/// *precisely when a run has already crashed. A truncated `checkpoint.json`
/// does not parse, so the one file a user needs after a crash was the one most
/// likely to be unreadable after one.
///
/// A second, atomic writer existed alongside this one for JSON session state,
/// which made the choice a coin flip for anyone adding a new state file. There
/// is now one writer and one guarantee; see [`write_restricted_atomic`].
pub fn write_restricted(path: &std::path::Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    write_restricted_atomic(path, contents)
}

/// Atomic + restricted write: temp file in the same directory + rename + 0600,
/// so state files are both crash-safe and secret-safe.
///
/// The temporary name is **unique per writer**. It used to be derived from the
/// target path alone, which meant two writers of the same path raced: one
/// renamed the temp file away while the other was still writing it, and the
/// loser's rename either failed or — worse — published the winner's
/// half-written bytes. Parallel coders writing into one task directory are
/// exactly that case.
///
/// Prefer [`write_restricted`]; this is the same function under a name that
/// predates the consolidation, kept because the two spellings are both in use.
pub fn write_restricted_atomic(
    path: &std::path::Path,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Same directory, so the rename stays within one filesystem and is atomic.
    // Unique, so concurrent writers cannot share it.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "niki".to_string());
    let tmp = path.with_file_name(format!(".{name}.{}.{unique}.tmp", std::process::id()));
    std::fs::write(&tmp, contents)?;
    #[cfg(unix)]
    {
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&tmp, perms)?;
    }
    // A failed rename must not leave the temp file behind: it would be a
    // second, stale copy of state next to the real one.
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// The former name of the atomic writer, kept as an alias so the three
/// existing call sites keep compiling and so no future caller can reach for the
/// non-atomic version by accident.
pub use write_restricted_atomic as write_atomic_restricted;

/// Stable, non-cryptographic 64-bit FNV-1a hash, hex-encoded. Used for local
/// content fingerprints (cache keys, workdir/config identity) where crypto
/// would add a dependency for no security benefit: these hashes detect
/// change, they do not authenticate.
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
