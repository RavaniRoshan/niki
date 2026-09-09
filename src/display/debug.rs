//! TUI debug surface (TUI-00D). Entirely gated by `NIKI_TUI_DEBUG`.
//!
//! - `NIKI_TUI_DEBUG=1` → append per-frame metadata to `./niki-tui-debug.log`.
//! - `NIKI_TUI_DEBUG=/path/to.log` → append there instead.
//! - Unset → every function below is a no-op with ~zero cost (one env lookup
//!   per frame; raw terminal writes are never intercepted).
//!
//! The log answers "why is the TUI redrawing / slow?" without a debugger:
//! each rendered frame records its rate target, draw time, dirty reason,
//! pipeline totals, and the engine's rolling mean/p95.

use std::io::Write;
use std::path::PathBuf;

/// Whether debug logging is enabled.
pub fn enabled() -> bool {
    std::env::var("NIKI_TUI_DEBUG").is_ok()
}

/// Resolve the log destination: the env value as a path, or the default file
/// when the value is flag-like (`1`, `true`, empty).
pub fn log_path() -> PathBuf {
    match std::env::var("NIKI_TUI_DEBUG") {
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true") || v.is_empty() => {
            PathBuf::from("niki-tui-debug.log")
        }
        Ok(v) => PathBuf::from(v),
        Err(_) => PathBuf::from("niki-tui-debug.log"),
    }
}

/// Append one per-frame record. Best-effort: I/O errors are swallowed so
/// debugging can never break the session it observes.
pub fn log_frame(
    frame_no: u64,
    target: &str,
    frame_ms: f64,
    dirty_reason: &str,
    stages: usize,
    tokens: usize,
    mean_ms: f64,
    p95_ms: f64,
) {
    if !enabled() {
        return;
    }
    let line = format!(
        "frame={frame_no} target={target} ms={frame_ms:.2} reason={dirty_reason} \
         stages={stages} tokens={tokens} mean={mean_ms:.2} p95={p95_ms:.2}\n"
    );
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_gating_and_log_line() {
        // SAFETY: single-threaded within this test; no other test touches
        // NIKI_TUI_DEBUG concurrently.
        let dir = std::env::temp_dir().join(format!("niki-dbg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.log");
        let _ = std::fs::remove_file(&path);

        unsafe { std::env::remove_var("NIKI_TUI_DEBUG") };
        assert!(!enabled());
        // Disabled → no file created even when called.
        log_frame(1, "High", 1.0, "key", 0, 0, 0.0, 0.0);
        assert!(!path.exists());

        unsafe { std::env::set_var("NIKI_TUI_DEBUG", &path) };
        assert!(enabled());
        assert_eq!(log_path(), path);
        log_frame(7, "Low", 2.5, "pipeline-event", 3, 1500, 2.0, 4.0);
        let content = std::fs::read_to_string(&path).expect("log written");
        assert!(content.contains("frame=7"), "{content}");
        assert!(content.contains("reason=pipeline-event"), "{content}");
        assert!(content.contains("tokens=1500"), "{content}");

        unsafe { std::env::remove_var("NIKI_TUI_DEBUG") };
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn debug_flag_values_resolve_to_default_path() {
        unsafe { std::env::set_var("NIKI_TUI_DEBUG", "1") };
        assert_eq!(log_path(), PathBuf::from("niki-tui-debug.log"));
        unsafe { std::env::remove_var("NIKI_TUI_DEBUG") };
    }
}
