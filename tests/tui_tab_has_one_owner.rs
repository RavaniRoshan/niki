//! `Tab` means one thing on each page, and it is not three things at once.
//!
//! Three controls claimed it:
//!
//! - the global `ToggleChatPage` binding (chat ↔ page view), checked **first**;
//! - `Config`'s `[Tab] next field` (`pages/config.rs:315`, `:327`); and
//! - `Agents`' `[Tab/Shift+Tab] prev/next` (`pages/agents.rs:294`, `:306`).
//!
//! The global won, so both page meanings were dead while both footers
//! advertised them. `Session` claims it too, through `handle_session_nav`.
//!
//! The rule now matches the one batch 2 and B3-01 established for `q`, `j` and
//! `k`: **the page that documents a key gets it; the global is the fallback.**
//! On those three pages `Tab` moves the field or the tab. Everywhere else it
//! still toggles chat and pages, which is where the muscle memory is and where
//! no page has an opinion.

use niki::config::types::NikiConfig;
use niki::display::pages::agents::AgentsPage;
use niki::display::pages::config::ConfigPage;
use niki::display::pages::{AppState, Page, PageId};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn state() -> AppState {
    AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn render(page: &mut dyn Page, state: &AppState) -> String {
    let mut term = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
    term.draw(|f| page.render(f, f.area(), state))
        .expect("draw");
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| {
                    let c = &buf[(x, y)];
                    let mark = if c.modifier.contains(ratatui::style::Modifier::BOLD) {
                        "\u{1}"
                    } else {
                        ""
                    };
                    format!("{}{}", c.symbol(), mark)
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plain(rendered: &str) -> String {
    rendered.replace('\u{1}', "")
}

/// Config's footer promises `Tab` moves the field, and the page must take the
/// key.
///
/// Deliberately **not** asserting that the selection is *visible*. It is not:
/// `ConfigPage::render` never reads `selected_field` (it is written at
/// `config.rs:328` and `:333` and read nowhere), so a render comparison here
/// would fail for a reason this slice does not fix. That is `ROADMAP.md` §1.10
/// — the selection cursor, and a `% 15` modulus against a form whose field
/// count has to be measured rather than assumed. Asserting the *routing* here
/// and the *cursor* there keeps both slices honestly scoped, which is the whole
/// reason this comment exists.
#[test]
fn config_takes_tab_rather_than_the_global_toggle() {
    let mut st = state();
    st.current_page = PageId::Config;
    let mut page = ConfigPage::new();

    let before = plain(&render(&mut page, &st));
    assert!(
        before.contains("Tab"),
        "the footer under test must advertise the key: {before}"
    );
    assert!(
        page.handle_key(key(KeyCode::Tab), &mut st),
        "`Tab` must be handled by the Config page, not swallowed by the global"
    );
    assert_eq!(
        st.current_page,
        PageId::Config,
        "taking `Tab` must not navigate away from the page that asked for it"
    );
    assert!(
        page.handle_key(key(KeyCode::BackTab), &mut st),
        "`Shift+Tab` must be handled too, or the user can only go forward"
    );
}

/// Agents' footer promises `Tab`/`Shift+Tab` move the tab. Same claim, same
/// page.
#[test]
fn tab_moves_the_agents_tab() {
    let mut st = state();
    st.current_page = PageId::Agents;
    // `AgentsPage` only cycles when there are stages to cycle through, and a
    // fresh state has none — so give it some, rather than asserting against a
    // no-op and calling it covered.
    st.stages = (0..3)
        .map(|i| niki::display::pages::StageInfo {
            role: niki::artifacts::types::AgentRole::Coder,
            status: niki::display::pages::StageStatus::Done,
            stream: String::new(),
            full_transcript: format!("transcript for stage {i}"),
            input_tokens: 10,
            output_tokens: 5,
            cost_usd: 0.001,
            latency_ms: 10,
            summary: vec![format!("summary {i}")],
            start: None,
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
        })
        .collect();
    let mut page = AgentsPage::new();

    let start = plain(&render(&mut page, &st));
    assert!(
        start.contains("Tab"),
        "the footer under test must advertise the key: {start}"
    );

    assert!(
        page.handle_key(key(KeyCode::Tab), &mut st),
        "`Tab` must be handled on the Agents page"
    );
    let moved = plain(&render(&mut page, &st));
    assert_ne!(
        start, moved,
        "`[Tab/Shift+Tab] prev/next` on Agents did nothing"
    );
}

/// The global must still work everywhere else — otherwise this slice has fixed
/// three broken footers by breaking the product.
#[test]
fn the_pages_that_claim_tab_are_the_only_ones() {
    let s = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");

    let arm = s
        .find("fn sub_page_owns(")
        .map(|i| s[i..].lines().take(24).collect::<String>())
        .expect("sub_page_owns exists");

    assert!(
        arm.contains("PageId::Config | PageId::Agents | PageId::Session"),
        "the three pages whose footers advertise `Tab` must be the ones it \
         yields to; a page that does not advertise it must still get the global"
    );
    // Both event loops must consult it before the global toggle, or one of
    // them still steals the key. Counted by anchoring on the toggle, not on
    // the whole file: `sub_page_owns` also guards the nav block, so counting
    // every occurrence found 3 and "failed" a correct file.
    let toggles = s.matches("GlobalAction::ToggleChatPage").count();
    assert_eq!(
        toggles, 2,
        "expected two `ToggleChatPage` arms (one per event loop); found {toggles}"
    );
    // Every `ToggleChatPage` check must be immediately followed by the guard.
    // Counted in pairs rather than by matching a formatted substring, because
    // rustfmt's line breaks are not a contract and the first version of this
    // assertion did exactly that and then counted the nav-block guards too.
    let guarded = s
        .lines()
        .filter(|l| l.contains("GlobalAction::ToggleChatPage"))
        .count();
    let guards_next_line = s
        .lines()
        .filter(|l| l.contains("GlobalAction::ToggleChatPage"))
        .all(|l| {
            // The guard is on the following line in both arms today.
            s.lines()
                .skip_while(|x| !x.contains(l.trim()))
                .nth(1)
                .is_some_and(|n| n.contains("!sub_page_owns"))
        });
    assert_eq!(guarded, toggles, "every toggle arm must be accounted for");
    assert!(
        guards_next_line,
        "a `ToggleChatPage` arm checks the key without deferring to the page, \
         so Config's and Agents' `Tab` are dead again"
    );
    // And the nav-block guards are still there — B3-03 must not have cost
    // B3-01 its coverage. Counted as "guards that are not toggle guards",
    // because the two kinds sit on lines of different shapes and a bare count
    // of the whole file is a number that moves every time another key joins
    // the predicate.
    let lines: Vec<&str> = s.lines().collect();
    let nav_guards = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("!sub_page_owns(key, &state)"))
        .filter(|(i, _)| {
            !lines[i.saturating_sub(1)..]
                .first()
                .is_some_and(|p| p.contains("ToggleChatPage"))
        })
        .count();
    assert_eq!(
        nav_guards, 2,
        "the two nav-block guards must survive; found {nav_guards}"
    );
    assert!(
        !arm.contains("KeyCode::Char('\\t')"),
        "`Tab` must be checked as a key code, not a letter"
    );
}
