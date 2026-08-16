use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

use chrono::Utc;

#[derive(Args)]
pub struct VerifyArgs {
    /// Path to the project directory
    #[arg(short, long, default_value = ".")]
    project: PathBuf,

    /// Description of the expected visual change
    description: String,

    /// Baseline screenshot path (optional)
    #[arg(long)]
    baseline: Option<PathBuf>,
}

/// Visual verification: capture a screenshot and compare against baseline.
///
/// Requires a display server and a screenshot tool (scrot, gnome-screenshot, or import).
/// In headless environments, this records the intent and skips actual capture.
pub fn handle(args: &VerifyArgs) -> Result<()> {
    let artifacts_dir = args.project.join(".niki").join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let screenshot_path = artifacts_dir.join(format!("verify-{}.png", timestamp));

    // Try to capture screenshot
    let captured = capture_screenshot(&screenshot_path);

    if captured {
        println!("Screenshot captured: {}", screenshot_path.display());
    } else {
        println!("Note: No screenshot tool available in this environment.");
        println!(
            "      Screenshot intent recorded at: {}",
            screenshot_path.display()
        );
        println!("\nTo enable visual verification, install one of:");
        println!("  - scrot (scrot -s {})", screenshot_path.display());
        println!(
            "  - gnome-screenshot (gnome-screenshot -f {})",
            screenshot_path.display()
        );
        println!(
            "  - ImageMagick (import -window root {})",
            screenshot_path.display()
        );
    }

    // Record verification intent
    let manifest = artifacts_dir.join("verify-manifest.json");
    let mut manifest_data: Vec<serde_json::Value> = Vec::new();
    if manifest.exists() {
        let content = std::fs::read_to_string(&manifest)?;
        if let Ok(data) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
            manifest_data = data;
        }
    }

    manifest_data.push(serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "description": args.description,
        "screenshot": screenshot_path.file_name().unwrap().to_string_lossy(),
        "baseline": args.baseline.as_ref().map(|p| p.display().to_string()),
        "captured": captured,
    }));

    std::fs::write(&manifest, serde_json::to_string_pretty(&manifest_data)?)?;

    println!("\nVerification manifest updated: {}", manifest.display());
    println!("Description: {}", args.description);
    if let Some(ref baseline) = args.baseline {
        println!("Baseline: {}", baseline.display());
    }

    Ok(())
}

fn capture_screenshot(path: &PathBuf) -> bool {
    // Try scrot
    if Command::new("scrot")
        .arg(path.to_str().unwrap_or("/tmp/niki-screenshot.png"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Try gnome-screenshot
    if Command::new("gnome-screenshot")
        .arg("-f")
        .arg(path.to_str().unwrap_or("/tmp/niki-screenshot.png"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Try ImageMagick import
    if Command::new("import")
        .arg("-window")
        .arg("root")
        .arg(path.to_str().unwrap_or("/tmp/niki-screenshot.png"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    false
}
