pub mod manifest;

pub use manifest::{RepoManifest, RiskSignal, build_manifest};

/// Manifest helpers shared with the KB builder (vendor exclusion + language
/// detection stay defined once, in `manifest.rs`).
pub(crate) fn manifest_language_for(rel: &std::path::Path) -> Option<&'static str> {
    manifest::language_for(rel)
}

pub(crate) fn manifest_vendor_dirs() -> &'static [&'static str] {
    manifest::VENDOR_DIRS
}
