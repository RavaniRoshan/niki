//! The first sixty seconds must be true and survivable.
//!
//! Onboarding is the first thing a new user sees: bare `niki` on a terminal
//! opens it. Three of its five pages asserted things the product does not do,
//! and below a certain size the whole surface went blank while still consuming
//! every keystroke.
//!
//! * The theme page offered `[1] Dark  [2] Light  [3] Colorblind`. The digits
//!   had no handler — the screen was byte-identical before and after pressing
//!   `1` — and `ThemeMode` is `Auto | Dark | Light`, so "Colorblind" was a
//!   theme that has never existed.
//! * The auth page said "Sign in with your provider (API key or OAuth)". There
//!   is no OAuth flow in the crate: `niki auth login` is a masked-prompt walk.
//!   It also named no command, on a page the user cannot act from, because the
//!   TUI owns stdin.
//! * The privacy page said "Niki collects anonymous usage data to improve the
//!   product… Telemetry is OFF by default." There is no collector in the
//!   binary, and README:299 says "No telemetry". A consent screen that
//!   contradicts the product's own documentation is worse than none.

use niki::display::onboarding::{OnboardingModal, OnboardingPage};

fn screen(modal: &OnboardingModal) -> String {
    let lines = modal.page_lines();
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn page_text(p: OnboardingPage) -> String {
    let mut m = OnboardingModal::new();
    m.page = p;
    screen(&m)
}

#[test]
fn onboarding_makes_no_claim_the_product_cannot_honour() {
    // The three that were false.
    assert!(
        !page_text(OnboardingPage::Welcome).contains("Colorblind"),
        "there is no Colorblind theme; ThemeMode is Auto | Dark | Light"
    );
    assert!(
        !page_text(OnboardingPage::AuthSecurity)
            .to_lowercase()
            .contains("oauth"),
        "there is no OAuth flow anywhere in the crate"
    );
    assert!(
        !page_text(OnboardingPage::Privacy)
            .to_lowercase()
            .contains("collects"),
        "the README promises no telemetry and the binary has no collector"
    );
}

#[test]
fn the_auth_page_names_a_command_the_user_can_run() {
    let text = page_text(OnboardingPage::AuthSecurity);
    assert!(
        text.contains("niki init"),
        "a user with no provider must be told the one command that fixes it:\\n{text}"
    );
    // And told they cannot do it from here, because the TUI owns stdin.
    assert!(
        text.contains("shell"),
        "the page must say where to run it:\\n{text}"
    );
}

#[test]
fn the_privacy_page_names_what_is_actually_contacted() {
    let text = page_text(OnboardingPage::Privacy);
    assert!(
        text.contains("model calls"),
        "the page should say what NIKI does contact:\\n{text}"
    );
    assert!(
        text.contains("OTLP") || text.contains("trace"),
        "an OTLP endpoint is a real outbound host and must be named:\\n{text}"
    );
}

/// A terminal too small to render must say so.
///
/// At 80x9 and 80x8 — both verified before the fix — the surface drew nothing
/// and the overlay ladder kept consuming every key, so the user got a black
/// void that ate their typing, with `Esc` as the only exit and nothing saying
/// so. Below 10 rows the onboarding modal cannot render at all, which is what
/// made this reachable on a real terminal.
#[test]
fn a_terminal_too_small_to_render_says_so_instead_of_going_blank() {
    use ratatui::backend::TestBackend;

    for (w, h) in [(80u16, 9u16), (80, 8), (40, 6), (20, 30)] {
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| niki::display::tui::render_too_small(f, f.area()))
            .expect("draw");
        let out = terminal.backend().to_string();
        assert!(
            out.contains("too small") || out.contains("NIKI needs at least"),
            "a {w}x{h} terminal produced a blank screen instead of an explanation:\\n{out}"
        );
    }
}

/// Every key the onboarding modal advertises must do something.
#[test]
fn every_advertised_onboarding_key_is_handled() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut m = OnboardingModal::new();
    for (label, key) in [
        ("1", KeyCode::Char('1')),
        ("2", KeyCode::Char('2')),
        ("3", KeyCode::Char('3')),
    ] {
        let before = screen(&m);
        let action = m.handle_key(KeyEvent::new(key, KeyModifiers::NONE));
        assert!(
            !before.contains(label),
            "onboarding must not advertise `{label}`; the screen says:\\n{before}"
        );
        let _ = action;
    }
    // The keys it does advertise all move the page.
    let mut m = OnboardingModal::new();
    let start = m.page;
    m.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_ne!(m.page, start, "[n] next must advance");
    m.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    assert_eq!(m.page, start, "[p] previous must go back");
}
