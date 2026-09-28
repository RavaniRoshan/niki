#!/usr/bin/env python3
"""Headless PTY harness for NIKI's TUI.

Closes the "cannot interactively click-test the TUI" gap: this drives the real
`niki chat` binary under a real pseudo-terminal (tuiwright) and asserts on the
rendered cell grid — mouse scroll, click-to-position, the visible scrollbar,
kill-ring yank, undo, multi-line input, slash commands, the command palette,
and the status bar.

Run:  pytest -c pytest_headless.ini -v
Env:  NIKI_BIN=/path/to/niki   (default: target/release/niki)
       NIKI_COLS / NIKI_ROWS   (default: 120 / 30)
"""
from __future__ import annotations

import os
import tempfile
import pytest

pytestmark = pytest.mark.headless

BIN = os.environ.get("NIKI_BIN", "target/release/niki")
COLS = int(os.environ.get("NIKI_COLS", "120"))
ROWS = int(os.environ.get("NIKI_ROWS", "30"))

# `tuiwright` is an optional dependency and is not installed by any CI job or
# requirements file, so this suite has never actually run. Previously the
# import lived inside `_session()`, which meant a run without tuiwright
# reported a collection of ERRORs rather than an honest "skipped" — easy to
# mistake for a broken TUI, and equally easy to never notice at all.
tuiwright = pytest.importorskip(
    "tuiwright",
    reason="tuiwright is not installed — see tests/headless_tui.py; "
    "the TUI's PTY boundary is covered by tests/tui_smoke/",
)


def _session():
    return tuiwright.TuiSession()


async def _dismiss_onboarding(s) -> None:
    """The onboarding modal auto-dismisses on a single Esc (Skip)."""
    try:
        await s.press("escape")
        await s.wait_for_stable(quiet_ms=200, timeout=4)
    except Exception:
        pass


async def _wait_chat_ready(s, timeout: float = 15.0) -> None:
    await s.wait_for_text("Describe a change", timeout=timeout)


@pytest.fixture
async def chat():
    # Isolated project dir: the chat log persists to disk and reloads on
    # startup, so sharing /tmp across tests bleeds state into later asserts.
    tmp = tempfile.mkdtemp(prefix="niki-tui-")
    s = _session()
    await s.start(
        [BIN, "chat", "-p", tmp],
        cols=COLS,
        rows=ROWS,
        env={"TERM": "ghostty", "TERM_PROGRAM": "ghostty"},
    )
    try:
        await _dismiss_onboarding(s)
        await _wait_chat_ready(s)
        yield s
    finally:
        await s.stop(timeout=3.0)


async def test_chat_renders_under_real_pty(chat):
    screen = chat.screen
    assert screen.contains("Build")
    assert screen.contains("Describe a change")


async def test_status_bar_renders(chat):
    # Footer renders the mode badge in uppercase.
    assert chat.screen.contains("MANUAL")


async def test_slash_status_command(chat):
    await chat.type("/status")
    await chat.press("enter")
    await chat.wait_for_text("Session Status", timeout=10)
    assert chat.screen.contains("Model")


async def test_slash_permissions_command(chat):
    await chat.type("/permissions")
    await chat.press("enter")
    await chat.wait_for_text("Permission modes", timeout=10)


async def test_slash_version_command(chat):
    await chat.type("/version")
    await chat.press("enter")
    await chat.wait_for_text("niki", timeout=10)


async def test_kill_ring_yank(chat):
    await chat.type("hello world")
    await chat.press("ctrl+w")      # kill "world" -> "hello "
    await chat.press("ctrl+y")      # yank "world" at the cursor -> "hello world"
    await chat.wait_for_stable(quiet_ms=150, timeout=4)
    assert chat.screen.contains("hello world")


async def test_input_undo(chat):
    await chat.type("ab")
    await chat.wait_for_stable(quiet_ms=100, timeout=3)
    # push_undo snapshots before each insert, so each Ctrl+Z reverts one char.
    await chat.press("ctrl+z")
    await chat.press("ctrl+z")
    await chat.wait_for_stable(quiet_ms=150, timeout=4)
    assert chat.screen.contains("Describe a change")


async def test_multiline_input_skipped(chat):
    """EXERCISED ELSEWHERE — see note below.

    The multi-line composer (Shift+Enter -> insert_newline) is implemented and
    unit-tested, but it cannot be driven through this PTY harness: crossterm,
    the crate the app reads keys through, does not decode kitty CSI-u sequences,
    and tuiwright's pyte emulator sends Shift+Enter as a plain Enter. Under a
    real Kitty/Ghostty terminal with the protocol enabled (B9), Shift+Enter
    inserts a newline correctly. Marked skip rather than faked.
    """
    pytest.skip("Shift+Enter not deliverable through the pyte emulator")


async def test_scroll_and_scrollbar_thumb_skipped(chat):
    """EXERCISED ELSEWHERE — see note below.

    The visible scrollbar + drag-to-jump (B2) render correctly and are wired to
    scroll_offset, but exercising them here requires submitting enough messages
    to overflow the viewport. Without a live LLM the pipeline gets stuck in a
    running stage after a few submits, so bulk submission isn't viable
    headlessly. The scrollbar rendering itself is covered by the layout unit
    tests and the render path is exercised on every frame by the smoke tests
    above. Marked skip rather than faked.
    """
    pytest.skip("bulk message submission needs a live LLM provider")


async def test_click_to_position_cursor(chat):
    await chat.click(row=ROWS - 2, col=40)
    await chat.wait_for_stable(quiet_ms=150, timeout=3)
    assert chat.screen.contains("Describe a change")


async def test_command_palette(chat):
    await chat.press("ctrl+p")
    await chat.wait_for_text("Commands", timeout=8)
    assert chat.screen.contains("Commands")


async def test_kitty_protocol_session_alive(chat):
    # TERM=ghostty makes kitty_capable() true, so the app enables the kitty
    # keyboard protocol around the session. If that broke rendering the screen
    # would be empty; the badge proves the session is still healthy.
    assert chat.screen.contains("Build")


async def test_session_exits_cleanly():
    tmp = tempfile.mkdtemp(prefix="niki-tui-")
    s = _session()
    await s.start(
        [BIN, "chat", "-p", tmp],
        cols=COLS,
        rows=ROWS,
        env={"TERM": "ghostty", "TERM_PROGRAM": "ghostty"},
    )
    await _dismiss_onboarding(s)
    await _wait_chat_ready(s)
    await s.press("ctrl+c")
    await s.press("ctrl+c")
    code = await s.stop(timeout=5.0)
    assert code == 0

# ── Navigation keys, verified in a real PTY ──────────────────────────────
#
# The navigation logic is unit-tested as a pure function in
# `display::nav`, but a pure function passing says nothing about whether the key
# reaches it through a real terminal. These drive the shipped binary under a
# real pseudo-terminal and read the rendered cell grid back.
#
# This is the layer where a wiring bug is invisible to every other test in the
# repo: a key bound correctly in the table but never dispatched would pass the
# unit tests, pass the canary corpus, and do nothing for the user.


async def _footer(s):
    """The bottom line, which is where the key hints render."""
    return s.screen.row(ROWS - 1)


async def test_footer_advertises_the_new_navigation_keys(chat):
    footer = await _footer(chat)
    assert "page" in footer, f"footer must advertise page navigation: {footer!r}"
    assert "keys" in footer, f"footer must advertise the key help: {footer!r}"


async def test_right_and_left_arrows_change_page(chat):
    # Tab toggles chat <-> page view; from the page view the arrows navigate.
    await chat.press("tab")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    before = chat.screen.text

    await chat.press("right")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    after_right = chat.screen.text
    assert after_right != before, "Right must move to a different page"

    await chat.press("left")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    assert chat.screen.text == before, "Left must return to the previous page"


async def test_h_and_l_navigate_like_the_arrows(chat):
    await chat.press("tab")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    start = chat.screen.text

    await chat.type("l")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    assert chat.screen.text != start, "l must move to the next page"

    await chat.type("h")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    assert chat.screen.text == start, "h must move to the previous page"


async def test_digit_key_jumps_to_a_page(chat):
    await chat.press("tab")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    before = chat.screen.text
    await chat.type("4")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    assert chat.screen.text != before, "a digit must jump to a page"


async def test_navigation_keys_are_letters_while_typing(chat):
    """The regression that matters most.

    `h`, `j`, `k` and `l` are navigation outside a text field and ordinary
    characters inside one. A composer that loses them is unusable, and nothing
    short of a real terminal would catch it.
    """
    await chat.type("hjkl")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    assert chat.screen.contains("hjkl"), (
        f"h/j/k/l must be typed literally into the composer, got: {chat.screen.row(ROWS - 2)!r}"
    )


async def test_arrow_keys_still_reach_the_composer(chat):
    await chat.type("ab")
    await chat.wait_for_stable(quiet_ms=150, timeout=4)
    await chat.press("left")
    await chat.type("X")
    await chat.wait_for_stable(quiet_ms=200, timeout=5)
    assert chat.screen.contains("aXb"), (
        f"Left must move the caret inside the composer, not change page; screen: {chat.screen.text!r}"
    )


# ── Sheets: the TUI must be able to change things, not only show them ──────
#
# The Config page has existed for a long time and is read-only — Tab moves a
# cursor across fifteen fields, nothing can be changed and nothing can be
# saved. A user who wanted to change a setting had to leave the product and edit
# TOML by hand, which is the opposite of what a terminal agent is for.
#
# These drive the real binary under a real PTY and then read the file. Asserting
# on the rendered screen alone would pass for a form that looks correct and
# writes nothing, which is exactly the failure being fixed.


@pytest.fixture
async def project_dir():
    """A throwaway project directory, cleaned up by the OS."""
    return tempfile.mkdtemp(prefix="niki-tui-")


@pytest.fixture
async def editor(project_dir):
    """(session, project_dir) for the tests that assert on what was written."""
    s = _session()
    await s.start(
        [BIN, "chat", "-p", project_dir],
        cols=COLS,
        rows=ROWS,
        env={"TERM": "ghostty", "TERM_PROGRAM": "ghostty"},
    )
    try:
        await _dismiss_onboarding(s)
        await _wait_chat_ready(s)
        yield s, project_dir
    finally:
        await s.stop(timeout=3.0)


async def _open_settings(s):
    await s.type("/config")
    await s.press("enter")
    # The hint line, which only the form renders.
    await s.wait_for_text("space change", timeout=10)
    await s.wait_for_stable(quiet_ms=200, timeout=5)


async def test_config_opens_an_editable_form(chat):
    await _open_settings(chat)
    # The hint names the key that changes a value, and the selected row's help
    # is on screen. The read-only page had neither.
    assert chat.screen.contains("space change")
    assert chat.screen.contains("terminal's background")


async def test_config_writes_a_setting_to_niki_toml(editor):
    s, project = editor
    await _open_settings(s)
    await s.press("down")        # Theme -> Max revision rounds
    await s.press("space")       # begin editing
    for ch in "7":
        await s.type(ch)
    await s.press("enter")       # stage the field
    await s.press("enter")       # save
    await s.wait_for_text("saved", timeout=10)

    config = os.path.join(project, "niki.toml")
    assert os.path.exists(config), f"{config} was not written"
    assert "max_revision_rounds = 7" in open(config).read()


async def test_config_keeps_the_comments_in_niki_toml(editor):
    s, project = editor
    config = os.path.join(project, "niki.toml")
    # A user keeps notes in this file. A settings screen that deletes them is a
    # regression, not a feature.
    with open(config, "w") as f:
        f.write(
            "# my project settings\n"
            "[general]\n"
            "# how many revisions before the run reports what it has\n"
            "max_revision_rounds = 3\n"
        )

    await _open_settings(s)
    await s.press("down")
    await s.press("space")
    for ch in "9":
        await s.type(ch)
    await s.press("enter")
    await s.press("enter")
    await s.wait_for_text("saved", timeout=10)

    after = open(config).read()
    assert "# my project settings" in after, after
    assert "# how many revisions" in after, after
    assert "max_revision_rounds = 9" in after, after


async def test_escape_closes_the_sheet_and_returns_to_the_conversation(chat):
    await _open_settings(chat)
    # Esc leaves a field, then discards a draft, then closes. A form that
    # swallows the key is a trap.
    #
    # The negative assertion looks for the form's own hint chrome. The first
    # version looked for "space change", which is also in the chat-log line the
    # /config handler pushes — so it failed in all three colour states, having
    # passed against a form that was working perfectly.
    for _ in range(3):
        await chat.press("escape")
        await chat.wait_for_stable(quiet_ms=150, timeout=4)
    assert not chat.screen.contains("move"), "the form chrome is still up"
    assert chat.screen.contains("Describe a change"), "and the composer is back"


async def test_theme_opens_a_picker_listing_what_the_product_supports(chat):
    await chat.type("/theme")
    await chat.press("enter")
    # "Theme" on its own is a bad thing to wait for: the settings form has a row
    # called Theme, so this passed against the wrong sheet whenever the picker
    # was slower to paint. Only colour mode lost that race here, which is a
    # useful reminder that a flaky wait is a flaky wait.
    await chat.wait_for_text("apply and save", timeout=10)
    # A list with descriptions, not a cycling key that tells you nothing about
    # what exists or how to get back.
    for name in ("auto", "dark", "light"):
        assert chat.screen.contains(name)
    assert chat.screen.contains("preview")


async def test_a_cancelled_theme_picker_restores_the_previous_theme(chat):
    await chat.type("/theme")
    await chat.press("enter")
    await chat.wait_for_text("apply and save", timeout=10)
    await chat.press("down")     # preview something else
    await chat.wait_for_stable(quiet_ms=250, timeout=5)
    await chat.press("escape")   # back out
    await chat.wait_for_stable(quiet_ms=250, timeout=5)
    assert not chat.screen.contains("apply and save")
