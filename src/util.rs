//! Secure file-writing helpers.
//!
//! Artifact files (reports, patches, audit logs, session/goal state) may contain
//! pipeline output or tool arguments that could include secrets. Writing them with
//! user-only permissions (0600) limits exposure on multi-user hosts. See research
//! report S11.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Write `contents` to `path` with user-only read/write permissions (0600), then
/// return the path on success. On non-Unix platforms the permission step is a
/// no-op (Windows ACLs are controlled separately).
pub fn write_restricted(path: &std::path::Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

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
