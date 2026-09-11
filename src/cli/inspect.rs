use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::config::NikiConfig;
use crate::repo_intel::build_manifest;

#[derive(Args)]
pub struct InspectArgs {
    /// Project directory to inspect (defaults to the current directory).
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Emit the manifest as JSON for automation.
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &InspectArgs) -> Result<()> {
    let project_dir = args
        .project
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    if !config.repo_intel.enabled {
        eprintln!("note: [repo_intel] is disabled — showing the baseline manifest anyway");
    }
    let manifest = build_manifest(&project_dir, &config);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }

    println!("Repository: {}", project_dir.display());
    println!();
    println!(
        "Languages: {}",
        if manifest.languages.is_empty() {
            "(none detected)".to_string()
        } else {
            manifest.languages.join(", ")
        }
    );
    println!(
        "Size: {} files, ~{} LOC{}",
        manifest.files,
        manifest.loc,
        if manifest.truncated {
            " (truncated at max_units)"
        } else {
            ""
        }
    );
    println!();
    print_list("Entry points", &manifest.entry_points);
    print_list("Tests", &manifest.test_paths);
    print_list("Config", &manifest.config_paths);
    print_list("Build files", &manifest.build_files);
    if !manifest.vendor_dirs.is_empty() {
        println!(
            "Vendor dirs (excluded): {}",
            manifest.vendor_dirs.join(", ")
        );
        println!();
    }
    if !manifest.package_info.is_empty() {
        println!("Dependencies:");
        for pkg in &manifest.package_info {
            println!(
                "  {} (`{}`): {}",
                pkg.manager,
                pkg.file_path,
                if pkg.dependencies.is_empty() {
                    "(none listed)".to_string()
                } else {
                    pkg.dependencies.join(", ")
                }
            );
        }
        println!();
    }
    if manifest.risk_signals.is_empty() {
        println!("Risk signals: none");
    } else {
        println!("Risk signals (advisory — config pattern hits):");
        for signal in &manifest.risk_signals {
            println!(
                "  [{}] `{}` in {}",
                signal.rule, signal.pattern, signal.detail
            );
        }
    }
    Ok(())
}

fn print_list(title: &str, items: &[String]) {
    if items.is_empty() {
        println!("{}: (none)", title);
    } else {
        println!("{}:", title);
        for item in items {
            println!("  - {}", item);
        }
    }
    println!();
}
