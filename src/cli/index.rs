use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::config::NikiConfig;
use crate::knowledge::structural::{build_index, index_root, open_index};

#[derive(Subcommand)]
pub enum IndexCommands {
    /// Build (or incrementally refresh) the structural symbol index.
    /// Idempotent: unchanged files hit the content-addressed cache.
    Build {
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
        /// Wipe the index and rebuild from scratch.
        #[arg(long)]
        full: bool,
        /// Report what would be indexed without writing anything.
        #[arg(long)]
        dry_stats: bool,
    },
    /// Query the index for a symbol: definition sites by default.
    Query {
        /// Symbol name (exact match).
        symbol: String,
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
        /// Show callers instead of definitions.
        #[arg(long)]
        callers: bool,
        /// Show callees instead of definitions.
        #[arg(long)]
        callees: bool,
    },
}

pub fn handle(command: &IndexCommands) -> Result<()> {
    match command {
        IndexCommands::Build {
            project,
            full,
            dry_stats,
        } => cmd_build(project.clone(), *full, *dry_stats),
        IndexCommands::Query {
            symbol,
            project,
            callers,
            callees,
        } => cmd_query(project.clone(), symbol, *callers, *callees),
    }
}

fn project_or_cwd(project: Option<PathBuf>) -> PathBuf {
    project.unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn cmd_build(project: Option<PathBuf>, full: bool, dry_stats: bool) -> Result<()> {
    let project_dir = project_or_cwd(project);
    let config = NikiConfig::load(&project_dir).unwrap_or_default();

    if dry_stats {
        let report = dry_stats_report(&project_dir, &config)?;
        println!(
            "Would index {} code files ({} units cached)",
            report.0, report.1
        );
        return Ok(());
    }

    if full {
        let root = index_root(&project_dir, &config);
        if root.exists() {
            std::fs::remove_dir_all(&root)?;
        }
    }
    let snapshot_id = crate::orchestrator::provenance::manual_snapshot_id(&project_dir);
    let report = build_index(&project_dir, &config, &snapshot_id)?;
    let m = &report.manifest;
    println!(
        "Index {:?} ({}): {} indexed, {} reused, {} skipped, {} coverage-only{}",
        m.status,
        m.resolver_version,
        report.written + report.reused,
        report.reused,
        m.units_skipped,
        m.units_coverage_only,
        if m.truncated {
            " (truncated at max_units)"
        } else {
            ""
        },
    );
    println!("Snapshot: {}", m.snapshot_id);
    Ok(())
}

/// (code files that would be indexed, units already cached).
fn dry_stats_report(project_dir: &PathBuf, config: &NikiConfig) -> Result<(usize, usize)> {
    let mut files = 0usize;
    let walker = walkdir::WalkDir::new(project_dir)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            if crate::repo_intel::manifest_vendor_dirs().contains(&name.as_ref()) {
                return false;
            }
            !name.starts_with('.')
        });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if matches!(
            ext.to_lowercase().as_str(),
            "rs" | "ts" | "tsx" | "mts" | "js" | "jsx" | "mjs" | "cjs" | "py" | "go"
        ) {
            files += 1;
        }
    }
    let cached = walkdir::WalkDir::new(index_root(project_dir, config).join("units"))
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().and_then(|e| e.to_str()) == Some("json")
        })
        .count();
    Ok((files, cached))
}

fn cmd_query(project: Option<PathBuf>, symbol: &str, callers: bool, callees: bool) -> Result<()> {
    let project_dir = project_or_cwd(project);
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    let index = match open_index(&project_dir, &config) {
        Ok(i) => i,
        Err(_) => {
            println!("No structural index found — run `niki index build` first.");
            println!("(Live search remains the source of truth either way.)");
            return Ok(());
        }
    };
    println!("Index snapshot: {}", index.state_ref);
    if callees {
        let sites = index.find_callees(symbol);
        if sites.is_empty() {
            println!("No indexed callees for `{symbol}` (advisory — try live search).");
        }
        for site in &sites {
            println!(
                "  {} calls {} ({}:{}, precision: {})",
                symbol,
                site.caller,
                site.file,
                site.line,
                site.precision.as_str()
            );
        }
    } else if callers {
        let sites = index.find_callers(symbol);
        if sites.is_empty() {
            println!("No indexed callers for `{symbol}` (advisory — try live search).");
        }
        for site in &sites {
            println!(
                "  {} called by {} ({}:{}, precision: {})",
                symbol,
                site.caller,
                site.file,
                site.line,
                site.precision.as_str()
            );
        }
    } else {
        let locs = index.locate_symbol(symbol);
        if locs.is_empty() {
            println!("`{symbol}` not in the index (advisory — try live search).");
        }
        for loc in &locs {
            println!(
                "  {} ({}:{} [{}], precision: {})",
                symbol,
                loc.file,
                loc.line,
                loc.kind,
                loc.precision.as_str()
            );
        }
    }
    Ok(())
}
