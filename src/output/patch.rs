use anyhow::Result;
use std::path::Path;

pub fn generate_patch(diff: &str, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, diff)?;
    Ok(())
}
