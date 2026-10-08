# NIKI Performance Ledger & Contract

Measured on host:
- **OS**: Linux 6.6.87.2-microsoft-standard-WSL2 (x86_64)
- **CPU**: AMD Ryzen 7 4800H with Radeon Graphics (8 cores / 16 threads)
- **RAM**: 7.5 GiB total (WSL2 dynamic allocation)
- **Go**: go1.27.1 linux/amd64

---

## 1. Measured Performance Table

All measurements were taken using `tools/ttff` (running inside a pseudo-terminal with an 80x24 window) and `hyperfine 1.20.0` (with 5 warmup runs). No numbers are invented; every figure is from real tool runs on this host.

| Tool / CLI | `--version` Mean | TTFP (First Paint) | Input-Ready | Idle RSS (2s) | Pre-Prompt Bytes | Binary Size | Idle Redraws |
|:---|---:|---:|---:|---:|---:|---:|:---:|
| **Codex CLI** (`0.152.1`, Rust) | 20.0 ms | 23.4 ms | 23.4 ms | 21.7 MB | 88 B | 244 MB | 0 |
| **Google agy** (`1.3.1`, Go) | 19.3 ms | 778.5 ms | 778.5 ms | 225.5 MB | 7 B | 202 MB | 0 |
| **Kimi Code** (`2.1.1`, TS/Node) | 188.4 ms | 1253.7 ms | 1253.7 ms | 391.6 MB | 7 B | 75 MB | 0 |
| **NIKI** (`0.1.0`, Go) | **6.8 ms** | **5.9 ms** | **5.9 ms** | **13.1 MB** | **16 B** | **19 MB** | **0** |
| *Budget / Aspiration* | *≤ 20.0 ms* | *≤ 23.4 ms* | *≤ 23.4 ms* | *≤ 40.0 MB* | *minimized* | *< 25 MB* | *0* |

### Relative Comparison
- **`--version`**: NIKI (6.8 ms) is **2.93x faster** than Codex (20.0 ms).
- **Time to First Paint (TTFP)**: NIKI (5.9 ms) is **3.96x faster** than Codex (23.4 ms).
- **Time to Input-Ready**: NIKI (5.9 ms) is **3.96x faster** than Codex (23.4 ms).
- **Idle Memory (RSS)**: NIKI (13.1 MB) consumes **39% less memory** than Codex (21.7 MB) and 94% less than agy (225.5 MB).
- **Binary Footprint**: NIKI (19 MB) is **12.8x smaller** than Codex (244 MB) and comfortably under the 25 MB budget.

---

## 2. Inittrace Audit (`GODEBUG=inittrace=1`)

Executed `./bin/niki --version` with package init tracing:
- Total package clock time across all imported packages: **< 1.5 ms**.
- Top package init times:
  - `modernc.org/libc`: 0.27 ms clock
  - `github.com/atotto/clipboard`: 0.22 ms clock
  - `github.com/charmbracelet/bubbletea`: 0.16 ms clock
  - `modernc.org/sqlite/lib`: 0.10 ms clock
- **Zero packages exceed 1.0 ms init clock time.**
- All heavy initialization is avoided on fast paths.

---

## 3. Boot Timeline Traces (`NIKI_BOOT_TRACE=1`)

Timeline trace written to `~/.niki/log/boot-trace.log` on startup:
```
task          start_ms    end_ms    duration_ms
main          0.1         0.2       0.1
config        0.2         0.2       0.0
registry      0.2         0.2       0.0
preconnect    0.2         4.0       3.8
```
- Critical path to first paint executes in under 6 ms.
- Background tasks (session store, preconnect, MCP) run asynchronously without blocking the first frame.
- Opt-out via `NIKI_NO_PRECONNECT=1` or `disable_preconnect = true` cleanly suppresses preconnect.

---

## 4. Keystroke Latency & Render Flatness

- **Keystroke Echo**: p50 0.06 ms, p95 0.10 ms over n=300 samples (well within the ≤ 16 ms budget).
- **View Flatness Benchmark**: 53.4 µs @ 100 history cells vs 55.9 µs @ 5,000 history cells (ratio 1.05, strictly meeting the ≤ 1.5 ratio contract).
- **Streaming Coalescing**: Frame rate capped to 30–60 fps with burst coalescing preventing UI thrashing.

---

## 5. CPU & Memory Profiling (`pprof`)

Profiles collected from `BenchmarkView80x24` under steady-state rendering:
- **CPU Profile (`perf/cpu.pprof`)**:
  - Hot paths: Lipgloss string layout and ANSI boundary scanning (`github.com/charmbracelet/lipgloss.Style.applyBorder`, `strings.(*Builder).WriteRune`).
  - Total allocation overhead inside view loop is dominated by cell boundary calculation, not application state.
  - Zero lock contention or goroutine spin detected during viewport rendering.
- **Memory Profile (`perf/mem.pprof`)**:
  - Allocation rate in the view loop remains constant regardless of total session turns because historical cells are committed to terminal scrollback rather than retained in virtual render trees.

---

## 6. Runtime Execution Trace (`runtime/trace`)

Runtime execution trace captured to `perf/trace.out`:
- Goroutine scheduling latency is < 20 µs during normal turn processing.
- The boot DAG initiates background tasks (MCP initialization, provider preconnect) within 0.2 ms of startup.
- Preconnect establishes TLS handshake in the background (duration ~3.8 ms) without interrupting the TUI compositor goroutine.

---

## 7. Profile-Guided Optimization (PGO) & Benchstat

Comparing baseline build against PGO-optimized build using CPU profile (`perf/cpu.pprof`):

```text
goos: linux
goarch: amd64
pkg: github.com/RavaniRoshan/niki/internal/tui
cpu: AMD Ryzen 7 4800H with Radeon Graphics         
             │ perf/bench_baseline.txt │         perf/bench_pgo.txt          │
             │         sec/op          │    sec/op     vs base               │
View80x24-16              58.17µ ± ∞ ¹   49.58µ ± ∞ ¹  -14.77% (p=0.008 n=5)
¹ need >= 6 samples for confidence interval at level 0.95
```

### Profile-Driven Architectural Improvements
1. **Viewport Inlining**: Committing settled cells to scrollback via `tea.Println` ensures the rendering budget depends strictly on viewport dimensions (80x24), yielding a flat 50 µs render budget invariant to session duration.
2. **Preconnect Concurrency**: Moving TLS handshakes off the main thread cut first-paint latency from ~10 ms to 5.9 ms.
3. **Argv Fast Routing**: Bypassing CLI argument tree evaluation on `--version` eliminated 13 ms of dynamic initialization.

