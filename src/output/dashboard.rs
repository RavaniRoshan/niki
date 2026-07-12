//! Static HTML dashboard (#7): a self-contained, offline diff viewer with
//! inline annotations sourced from the Reviewer's issues and the Security
//! Auditor's findings.
//!
//! We deliberately emit a single static `dashboard.html` rather than run a web
//! server: it works offline, needs no extra runtime dependency, and can be
//! committed or shared. The run pipeline writes it next to `report.md`, and the
//! `niki dashboard` subcommand can (re)generate it from persisted artifacts.

use anyhow::Result;
use std::path::Path;

use crate::artifacts::types::{ReviewVerdict, SecurityVerdict};

/// One rendered annotation attached to a file (and optionally a line range).
struct Annotation {
    source: &'static str, // "Review" | "Security"
    severity: String,
    category: String,
    file: Option<String>,
    line_range: Option<String>,
    description: String,
    suggested_fix: Option<String>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Classify a unified-diff line into a CSS class for coloring.
fn line_class(line: &str) -> &'static str {
    if line.starts_with("+++") || line.starts_with("---") {
        "diff-meta"
    } else if line.starts_with("@@") {
        "diff-hunk"
    } else if line.starts_with('+') {
        "diff-add"
    } else if line.starts_with('-') {
        "diff-del"
    } else if line.starts_with("diff ") || line.starts_with("index ") {
        "diff-meta"
    } else {
        "diff-ctx"
    }
}

fn render_diff(diff: &str) -> String {
    if diff.trim().is_empty() {
        return "<p class=\"empty\">No changes were produced.</p>".to_string();
    }
    let mut out = String::from("<pre class=\"diff\">");
    for line in diff.lines() {
        let cls = line_class(line);
        out.push_str(&format!(
            "<span class=\"{}\">{}</span>\n",
            cls,
            esc(line)
        ));
    }
    out.push_str("</pre>");
    out
}

fn annotations_from_review(json: &str) -> Vec<Annotation> {
    let mut out = Vec::new();
    if let Ok(v) = serde_json::from_str::<ReviewVerdict>(json) {
        for issue in v.issues {
            out.push(Annotation {
                source: "Review",
                severity: format!("{:?}", issue.severity),
                category: format!("{:?}", issue.category),
                file: issue.file_path,
                line_range: issue.line_range,
                description: issue.description,
                suggested_fix: issue.suggested_fix,
            });
        }
    }
    out
}

fn annotations_from_security(json: &str) -> Vec<Annotation> {
    let mut out = Vec::new();
    if let Ok(v) = serde_json::from_str::<SecurityVerdict>(json) {
        for f in v.findings {
            out.push(Annotation {
                source: "Security",
                severity: format!("{:?}", f.severity),
                category: format!("{:?}", f.category),
                file: f.file_path,
                line_range: f.line_range,
                description: f.description,
                suggested_fix: f.suggested_fix,
            });
        }
    }
    out
}

fn severity_rank(sev: &str) -> u8 {
    match sev.to_lowercase().as_str() {
        "critical" => 0,
        "high" | "major" => 1,
        "medium" | "minor" => 2,
        "low" | "nit" => 3,
        _ => 4,
    }
}

fn render_annotations(anns: &[Annotation]) -> String {
    if anns.is_empty() {
        return "<p class=\"empty\">No annotations.</p>".to_string();
    }
    let mut sorted: Vec<&Annotation> = anns.iter().collect();
    sorted.sort_by_key(|a| severity_rank(&a.severity));

    let mut out = String::new();
    for a in sorted {
        let loc = match (&a.file, &a.line_range) {
            (Some(f), Some(l)) => format!("{}:{}", esc(f), esc(l)),
            (Some(f), None) => esc(f),
            _ => "—".to_string(),
        };
        let sev_class = format!("sev-{}", severity_rank(&a.severity));
        out.push_str(&format!(
            "<div class=\"ann {sev_class}\">\
               <div class=\"ann-head\">\
                 <span class=\"badge\">{src}</span>\
                 <span class=\"sev\">{sev}</span>\
                 <span class=\"cat\">{cat}</span>\
                 <span class=\"loc\">{loc}</span>\
               </div>\
               <div class=\"ann-body\">{desc}</div>",
            sev_class = sev_class,
            src = a.source,
            sev = esc(&a.severity),
            cat = esc(&a.category),
            loc = loc,
            desc = esc(&a.description),
        ));
        if let Some(fix) = &a.suggested_fix {
            out.push_str(&format!("<div class=\"ann-fix\">💡 {}</div>", esc(fix)));
        }
        out.push_str("</div>");
    }
    out
}

const STYLE: &str = r#"
:root{--bg:#0d1117;--panel:#161b22;--fg:#e6edf3;--sub:#8b949e;--border:#30363d;
--add-bg:#12261e;--add-fg:#3fb950;--del-bg:#25171c;--del-fg:#f85149;--hunk:#a5a5ff;--meta:#8b949e;}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;}
header{padding:20px 28px;border-bottom:1px solid var(--border);background:var(--panel);}
header h1{margin:0 0 4px;font-size:18px;}
header .meta{color:var(--sub);font-size:13px;}
.wrap{display:grid;grid-template-columns:1fr 380px;gap:0;height:calc(100vh - 74px);}
.col{overflow:auto;padding:20px 24px;}
.col.diffcol{border-right:1px solid var(--border);}
h2{font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--sub);margin:0 0 12px;}
pre.diff{margin:0;font:12.5px/1.5 "SF Mono",ui-monospace,Menlo,Consolas,monospace;white-space:pre;
  background:var(--panel);border:1px solid var(--border);border-radius:8px;padding:14px;overflow-x:auto;}
pre.diff span{display:block;padding:0 6px;border-radius:2px;}
.diff-add{background:var(--add-bg);color:var(--add-fg);}
.diff-del{background:var(--del-bg);color:var(--del-fg);}
.diff-hunk{color:var(--hunk);}
.diff-meta{color:var(--meta);}
.diff-ctx{color:var(--fg);}
.empty{color:var(--sub);font-style:italic;}
.ann{background:var(--panel);border:1px solid var(--border);border-left-width:4px;border-radius:8px;padding:10px 12px;margin-bottom:10px;}
.ann-head{display:flex;flex-wrap:wrap;gap:8px;align-items:center;font-size:12px;margin-bottom:6px;}
.badge{background:#1f6feb;color:#fff;border-radius:4px;padding:1px 6px;font-size:11px;}
.ann .sev{font-weight:700;text-transform:uppercase;font-size:11px;}
.ann .cat{color:var(--sub);}
.ann .loc{margin-left:auto;color:var(--sub);font-family:ui-monospace,monospace;}
.ann-body{font-size:13px;}
.ann-fix{margin-top:6px;font-size:12.5px;color:var(--add-fg);}
.sev-0{border-left-color:#f85149}.sev-0 .sev{color:#f85149}
.sev-1{border-left-color:#db6d28}.sev-1 .sev{color:#db6d28}
.sev-2{border-left-color:#d29922}.sev-2 .sev{color:#d29922}
.sev-3{border-left-color:#3fb950}.sev-3 .sev{color:#3fb950}
.sev-4{border-left-color:#8b949e}.sev-4 .sev{color:#8b949e}
.metrics{width:100%;border-collapse:collapse;font-size:12.5px;margin-top:6px;}
.metrics th,.metrics td{border:1px solid var(--border);padding:5px 8px;text-align:right;}
.metrics th:first-child,.metrics td:first-child{text-align:left;}
.metrics th{color:var(--sub);font-weight:600;}
"#;

/// Inputs for building the dashboard, independent of the pipeline types so the
/// `dashboard` subcommand can build it straight from persisted artifacts.
pub struct DashboardInput<'a> {
    pub task_id: &'a str,
    pub description: &'a str,
    pub verdict: &'a str,
    pub revision_rounds: u32,
    pub final_diff: &'a str,
    /// Raw review_verdict.json, if present.
    pub review_json: Option<&'a str>,
    /// Raw security_auditor.json, if present.
    pub security_json: Option<&'a str>,
    /// Rendered "Cost & Performance" rows as (label, value) pairs.
    pub metrics_rows: Vec<(String, String)>,
}

pub fn render_html(input: &DashboardInput) -> String {
    let mut anns = Vec::new();
    if let Some(j) = input.review_json {
        anns.extend(annotations_from_review(j));
    }
    if let Some(j) = input.security_json {
        anns.extend(annotations_from_security(j));
    }

    let metrics_html = if input.metrics_rows.is_empty() {
        String::new()
    } else {
        let mut t = String::from(
            "<h2>Cost &amp; Performance</h2><table class=\"metrics\"><tr><th>Metric</th><th>Value</th></tr>",
        );
        for (k, v) in &input.metrics_rows {
            t.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>", esc(k), esc(v)));
        }
        t.push_str("</table>");
        t
    };

    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>NIKI · {title}</title><style>{style}</style></head><body>\
<header><h1>NIKI Dashboard</h1>\
<div class=\"meta\">Task {task} · verdict <b>{verdict}</b> · {rounds} revision round(s)<br>{desc}</div>\
</header>\
<div class=\"wrap\">\
<div class=\"col diffcol\"><h2>Diff</h2>{diff}</div>\
<div class=\"col\"><h2>Annotations ({ann_count})</h2>{anns}{metrics}</div>\
</div></body></html>",
        title = esc(input.task_id),
        style = STYLE,
        task = esc(input.task_id),
        verdict = esc(input.verdict),
        rounds = input.revision_rounds,
        desc = esc(input.description),
        diff = render_diff(input.final_diff),
        ann_count = anns.len(),
        anns = render_annotations(&anns),
        metrics = metrics_html,
    )
}

/// Write `dashboard.html` into `task_dir`, returning its path.
pub fn write_dashboard(task_dir: &Path, input: &DashboardInput) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(task_dir)?;
    let path = task_dir.join("dashboard.html");
    std::fs::write(&path, render_html(input))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_diff_line_classes() {
        assert_eq!(line_class("+added"), "diff-add");
        assert_eq!(line_class("-removed"), "diff-del");
        assert_eq!(line_class("@@ -1 +1 @@"), "diff-hunk");
        assert_eq!(line_class(" context"), "diff-ctx");
    }

    #[test]
    fn escapes_html_in_diff() {
        let html = render_diff("+let x = a<b && c>d;");
        assert!(html.contains("&lt;b"));
        assert!(!html.contains("<b &&"));
    }

    #[test]
    fn parses_review_annotations() {
        let json = r#"{
            "verdict":"revision_needed","overall_assessment":"x",
            "quality_scores":{"correctness":5,"code_quality":5,"test_coverage":5,"spec_adherence":5},
            "issues":[{"severity":"critical","category":"bug","description":"npe","file_path":"a.rs","line_range":"1-2","suggested_fix":"guard"}],
            "strengths":[]
        }"#;
        let anns = annotations_from_review(json);
        assert_eq!(anns.len(), 1);
        assert_eq!(anns[0].severity, "Critical");
    }

    #[test]
    fn full_html_contains_sections() {
        let input = DashboardInput {
            task_id: "abc",
            description: "do a thing",
            verdict: "Approved",
            revision_rounds: 1,
            final_diff: "+hello",
            review_json: None,
            security_json: None,
            metrics_rows: vec![("Total cost".into(), "$0.01".into())],
        };
        let html = render_html(&input);
        assert!(html.contains("NIKI Dashboard"));
        assert!(html.contains("Annotations (0)"));
        assert!(html.contains("Total cost"));
    }
}
