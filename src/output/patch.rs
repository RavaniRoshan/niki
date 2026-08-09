use anyhow::Result;
use std::path::Path;

pub fn generate_patch(diff: &str, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::util::write_restricted(output_path, diff)?;
    Ok(())
}
