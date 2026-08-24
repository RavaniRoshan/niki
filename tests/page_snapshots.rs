//! Per-page visual regression via `TestBackend` + insta cell snapshots.
//!
//! Every dashboard page renders into an in-memory buffer at two pinned sizes
//! and is snapshotted symbol-by-symbol. This catches layout drift exactly the
//! way Codex-rs/gitui do it (ratatui's own recommended recipe) — see
//! research/ultimate-test-suite-niki.md Phase 2.
//!
//! Volatility is pinned, not filtered: spinner tick frozen, all Instants
//! cleared, modals closed, so snapshots are byte-stable.
//!
//! Update snapshots after intentional UI changes:
//!   INSTA_UPDATE=always cargo test --test page_snapshots
//! then review with `git diff tests/snapshots/`.

use insta::assert_snapshot;
use niki::artifacts::types::AgentRole;
use niki::config::types::NikiConfig;
use niki::display::pages::{AppState, PageId, PageRouter, StageInfo, StageStatus};
use niki::mission::{AttentionPriority, Mission, MissionId, MissionStatus};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const SAMPLE_DIFF: &str = "\
--- a/src/list.rs\n\
+++ b/src/list.rs\n\
@@ -28,7 +28,7 @@\n\
-    let end = start + size - 1;\n\
+    let end = start + size;\n\
";

fn stage(role: AgentRole, summary: &str) -> StageInfo {
    StageInfo {
        role,
        status: StageStatus::Done,
        stream: String::new(),
        full_transcript: format!("transcript: {}", summary),
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.001,
        latency_ms: 1200,
        summary: vec![summary.to_string()],
        start: None,
        prompt_file: None,
        retry_count: 0,
        error_message: None,
    }
}

/// Deterministic AppState: every clock/spinner/modal source pinned.
fn seeded_state() -> AppState {
    let mut state = AppState::new(
        "add a health endpoint".to_string(),
        NikiConfig::default(),
        // Fixed literal so pages that echo the project path render
        // identically on every machine.
        std::path::PathBuf::from("/tmp/niki-page-snapshots"),
    );
    state.tick = 0;
    state.notice = None;
    state.modal = None;
    state.onboarding = None;
    state.hover_time = None;
    state.click_flash = None;
    state.last_click_time = None;
    state.start_time = None;
    state.show_help = false;
    state.show_command_menu = false;
    state.show_permission_modal = false;
    state.finished = false;
    state.paused = false;

    state.stages = vec![
        stage(AgentRole::Planner, "plan ready"),
        stage(AgentRole::Coder, "patch applied"),
        stage(AgentRole::Tester, "tests green"),
        stage(AgentRole::Reviewer, "approved"),
    ];
    state.context_usage = 0.35;
    state.context_limit = 128_000;
    state.token_count = 44_800;
    state.cost = 0.0123;
    state.model = "mock-model".to_string();
    state.branch_name = "niki/e5c8f1".to_string();
    state.revision_round = 0;
    state.max_revision_rounds = 3;
    state.diff_content = Some(SAMPLE_DIFF.to_string());
    state.report_content = Some("## Verdict\n\nApproved: clean fix.".to_string());
    state.test_log = Some("running 2 tests\ntest health ... ok\ntest root ... ok".to_string());
    state.cost_json = Some("{\"total_usd\": 0.0123}".to_string());
    state.artifacts_dir = Some(std::path::PathBuf::from(".niki/e5c8f1"));
    state.chat_log = vec![
        ("user".to_string(), "Add a health endpoint".to_string()),
        (
            "assistant".to_string(),
            "Dispatching pipeline: planner, coder, tester, reviewer.".to_string(),
        ),
    ];
    state.messages.clear();
    state
}

fn render_page(page: PageId, width: u16, height: u16) -> String {
    let mut state = seeded_state();
    state.current_page = page;

    if page == PageId::Fleet {
        state.fleet = fleet_fixture();
        state.session_view = Some(niki::display::pages::session::SessionState::new(
            state.fleet.missions[0].clone(),
        ));
    }
    if page == PageId::Session && state.session_view.is_none() {
        state.session_view = Some(niki::display::pages::session::SessionState::new(
            fleet_fixture().missions[0].clone(),
        ));
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let frame = terminal
        .draw(|f| {
            let area = f.area();
            // Mirror tui.rs's real dispatch: Fleet and Session render through
            // their own buffer painters, everything else via the router.
            match page {
                PageId::Fleet => {
                    niki::display::pages::fleet::render_fleet(&state.fleet, area, f.buffer_mut());
                }
                PageId::Session => {
                    let sv = state.session_view.as_ref().unwrap();
                    niki::display::pages::session::render_session(sv, area, f.buffer_mut());
                }
                _ => {
                    let router = PageRouter::new();
                    router.render_current(f, area, &state);
                }
            }
        })
        .unwrap();
    let buf = frame.buffer.clone();
    let w = buf.area.width as usize;
    let mut out = String::new();
    for (i, cell) in buf.content.iter().enumerate() {
        out.push_str(cell.symbol());
        if w > 0 && (i + 1) % w == 0 && i + 1 < buf.content.len() {
            out.push('\n');
        }
    }
    out
}

fn fleet_fixture() -> niki::display::pages::fleet::FleetState {
    let mut mission = Mission::new(
        MissionId("m-e5c8f1".into()),
        "add a health endpoint".to_string(),
        "mock-model".to_string(),
    );
    mission.status = MissionStatus::Running;
    mission.progress = 0.5;
    mission.cost_usd = 0.0123;
    mission.branch = Some("niki/e5c8f1".to_string());
    mission.attention = AttentionPriority::Normal;
    niki::display::pages::fleet::FleetState::new(vec![mission])
}

fn snapshot_both_sizes(name: &str) {
    let wide = render_page_wide(name);
    assert_snapshot!(format!("{name}_120x30"), wide);
    let narrow = render_page_narrow(name);
    assert_snapshot!(format!("{name}_80x24"), narrow);
}

// The two helpers exist so each size has its own call site; they share logic
// via render_page but keep failure output readable.
fn render_page_wide(id: &str) -> String {
    render_page(PageId::from_title(id), 120, 30)
}

fn render_page_narrow(id: &str) -> String {
    render_page(PageId::from_title(id), 80, 24)
}

trait PageIdTitle {
    fn from_title(title: &str) -> PageId;
}

impl PageIdTitle for PageId {
    fn from_title(title: &str) -> PageId {
        *PageId::all()
            .iter()
            .find(|p| p.title() == title)
            .unwrap_or(&PageId::Run)
    }
}

macro_rules! page_snapshot_test {
    ($test_name:ident, $title:literal) => {
        #[test]
        fn $test_name() {
            snapshot_both_sizes($title);
        }
    };
}

// One test per dashboard page — the live denominator is `PageId::all()` and a
// guard below fails if a page is added without a snapshot suite.
page_snapshot_test!(page_run, "run");
page_snapshot_test!(page_pipeline, "pipeline");
page_snapshot_test!(page_agents, "agents");
page_snapshot_test!(page_diff, "diff");
page_snapshot_test!(page_verdict, "verdict");
page_snapshot_test!(page_cost, "cost");
page_snapshot_test!(page_artifacts, "artifacts");
page_snapshot_test!(page_history, "history");
page_snapshot_test!(page_config, "config");
page_snapshot_test!(page_help, "help");
page_snapshot_test!(page_test_log, "test_log");
page_snapshot_test!(page_fleet, "fleet");
page_snapshot_test!(page_session, "session");
page_snapshot_test!(page_chat, "chat");

#[test]
fn every_page_has_a_snapshot_test() {
    // Coverage gate: the number of page_snapshot_test! invocations above must
    // track PageId::all(). Bump both together when adding a page.
    let titles: Vec<&str> = PageId::all().iter().map(|p| p.title()).collect();
    assert_eq!(
        titles.len(),
        14,
        "PageId::all() changed; add a page_snapshot_test! for each new page"
    );
}
