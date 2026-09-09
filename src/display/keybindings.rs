//! Central keybinding table (TUI-003).
//!
//! Semantic [`GlobalAction`]s are matched at the global dispatch level in
//! `tui.rs`; page-local and input-editing keys stay receiver-scoped (flat IDs
//! + receiver priority, following the Pi extraction findings). Defaults
//! reproduce the historical literals exactly, including their modifier
//! quirks (`g`/`s`/`?` ignore modifiers; `Tab` forbids only Ctrl).
//!
//! Users may override per-action keys via `[ui.keybindings]` in `niki.toml`:
//!
//! ```toml
//! [ui.keybindings]
//! command_palette = ["ctrl+p", "ctrl+k"]
//! ```
//!
//! Only user-layer conflicts are reported ([`Conflict`]); default-vs-default
//! collisions would be a bug (asserted in tests), never user intent.
//!
//! Scope note: the table drives `run_tui` and the keys `run_chat` shares with
//! it (`?`, `Ctrl+E`). `run_chat` keeps divergent literals (bare-`t` theme,
//! `q` quit) until its dispatch is unified — see `tui.rs`.

use std::collections::HashMap;
use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key combination plus its modifier rule.
///
/// `require` must be contained in the event modifiers, `forbid` must not
/// intersect them. An empty/empty rule matches the code regardless of
/// modifiers — this preserves the historical `g`/`s`/`?` behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub code: KeyCode,
    pub require: KeyModifiers,
    pub forbid: KeyModifiers,
}

impl KeyCombo {
    fn matches(&self, key: &KeyEvent) -> bool {
        key.code == self.code
            && key.modifiers.contains(self.require)
            && !key.modifiers.intersects(self.forbid)
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.require.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.require.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if self.require.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }
        let key = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            // Canonical display: Ctrl+P style (specs are case-insensitive).
            KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "Shift+Tab".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::F(n) => return write!(f, "{}{}", parts.join("+"), format!("F{n}")),
            _ => "?".to_string(),
        };
        parts.push(&key);
        write!(f, "{}", parts.join("+"))
    }
}

/// Parse `"ctrl+p"`, `"?"`, `"tab"`, `"esc"`, `"f5"`, `"up"` etc.
/// Modifiers (`ctrl`/`alt`/`shift`, in any order) precede the key with `+`.
/// Returns `None` for unknown names. The parsed combo requires exactly the
/// given modifiers (forbids nothing); table defaults may widen this.
pub fn parse_combo(spec: &str) -> Option<KeyCombo> {
    let mut require = KeyModifiers::empty();
    let mut parts: Vec<&str> = spec.split('+').collect();
    let key = parts.pop()?.trim().to_lowercase();
    for m in parts {
        match m.trim().to_lowercase().as_str() {
            "ctrl" | "control" => require |= KeyModifiers::CONTROL,
            "alt" | "meta" => require |= KeyModifiers::ALT,
            "shift" => require |= KeyModifiers::SHIFT,
            _ => return None,
        }
    }
    let code = match key.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        s if s.len() == 1 => KeyCode::Char(s.chars().next()?),
        s if s.starts_with('f') => {
            let n: u8 = s[1..].parse().ok()?;
            if !(1..=12).contains(&n) {
                return None;
            }
            KeyCode::F(n)
        }
        _ => return None,
    };
    Some(KeyCombo {
        code,
        require,
        forbid: KeyModifiers::empty(),
    })
}

/// Semantic actions matched at the global dispatch level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalAction {
    ToggleHelp,
    ToggleMouseCapture,
    CancelOrExit,
    CommandPalette,
    CycleTheme,
    ToggleChatPage,
    GotoFleet,
    GotoSession,
}

/// One row of the binding table.
pub struct BindingDef {
    pub id: &'static str,
    pub description: &'static str,
    pub action: Option<GlobalAction>,
    /// (key spec, require, forbid) — specs parsed by [`parse_combo`].
    pub defaults: &'static [(&'static str, KeyModifiers, KeyModifiers)],
}

const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const NONE: KeyModifiers = KeyModifiers::empty();

/// The full table: runtime-matched actions first, help-only rows after.
/// Order is match priority AND help order.
pub static BINDING_TABLE: &[BindingDef] = &[
    BindingDef {
        id: "toggle_help",
        description: "Toggle keybinding help",
        action: Some(GlobalAction::ToggleHelp),
        defaults: &[("?", NONE, NONE)],
    },
    BindingDef {
        id: "toggle_mouse",
        description: "Toggle mouse capture (text selection)",
        action: Some(GlobalAction::ToggleMouseCapture),
        defaults: &[("ctrl+e", CTRL, NONE)],
    },
    BindingDef {
        id: "cancel_exit",
        description: "Cancel running stage / exit",
        action: Some(GlobalAction::CancelOrExit),
        defaults: &[("ctrl+c", CTRL, NONE)],
    },
    BindingDef {
        id: "command_palette",
        description: "Open the command palette",
        action: Some(GlobalAction::CommandPalette),
        defaults: &[("ctrl+p", CTRL, NONE)],
    },
    BindingDef {
        id: "cycle_theme",
        description: "Cycle theme (dark → light → auto)",
        action: Some(GlobalAction::CycleTheme),
        defaults: &[("ctrl+t", CTRL, NONE)],
    },
    BindingDef {
        id: "toggle_chat",
        description: "Switch chat ↔ page view",
        action: Some(GlobalAction::ToggleChatPage),
        defaults: &[("tab", NONE, CTRL)],
    },
    BindingDef {
        id: "goto_fleet",
        description: "Jump to the Fleet grid",
        action: Some(GlobalAction::GotoFleet),
        defaults: &[("g", NONE, NONE)],
    },
    BindingDef {
        id: "goto_session",
        description: "Open the Session view",
        action: Some(GlobalAction::GotoSession),
        defaults: &[("s", NONE, NONE)],
    },
    // Help-only rows (receiver-scoped keys, not globally matched).
    BindingDef {
        id: "doc_submit",
        description: "Submit input / run command",
        action: None,
        defaults: &[("enter", NONE, NONE)],
    },
    BindingDef {
        id: "doc_close",
        description: "Close menu, modal, or overlay",
        action: None,
        defaults: &[("esc", NONE, NONE)],
    },
    BindingDef {
        id: "doc_clear",
        description: "Clear the screen",
        action: None,
        defaults: &[("ctrl+l", CTRL, NONE)],
    },
    BindingDef {
        id: "doc_history",
        description: "History navigation / menu navigation",
        action: None,
        defaults: &[("up", NONE, NONE)],
    },
    BindingDef {
        id: "doc_line",
        description: "Jump to line start / end",
        action: None,
        defaults: &[("ctrl+a", CTRL, NONE)],
    },
    BindingDef {
        id: "doc_kill",
        description: "Delete word / to line start / end",
        action: None,
        defaults: &[("ctrl+w", CTRL, NONE)],
    },
    BindingDef {
        id: "doc_prefix",
        description: "File / slash-command / shell prefix",
        action: None,
        defaults: &[("@", NONE, NONE)],
    },
    BindingDef {
        id: "doc_search",
        description: "Reverse history search",
        action: None,
        defaults: &[("ctrl+r", CTRL, NONE)],
    },
];

/// Documentary rows whose label differs from the combo display.
fn doc_label(id: &str, combos: &[KeyCombo]) -> String {
    match id {
        "doc_history" => "↑ / ↓".to_string(),
        "doc_line" => "Ctrl+A / E".to_string(),
        "doc_kill" => "Ctrl+W / U / K".to_string(),
        "doc_prefix" => "@ / / / !".to_string(),
        _ => combos
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// A user-layer binding problem found while applying overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// `[ui.keybindings]` names an action id that does not exist.
    UnknownId(String),
    /// A key spec that does not parse; the default combos are kept.
    BadKey { id: String, key: String },
    /// Two actions claim the same combo; table order wins.
    Clash {
        combo: String,
        kept: &'static str,
        dropped: &'static str,
    },
}

/// Resolved bindings: ordered (combo, action) pairs, first match wins.
#[derive(Debug, Clone, Default)]
pub struct KeyBindings {
    map: Vec<(KeyCombo, GlobalAction)>,
}

impl KeyBindings {
    /// Defaults exactly as historically handled (verified against `tui.rs`).
    pub fn defaults() -> Self {
        let (b, conflicts) = Self::with_overrides(&HashMap::new());
        debug_assert!(conflicts.is_empty(), "default table clashes: {conflicts:?}");
        b
    }

    /// Apply `[ui.keybindings]` overrides (`id → [key specs]`).
    /// An id's combos are replaced wholesale; unparseable specs are skipped
    /// (defaults kept when nothing valid remains); later table rows lose
    /// combos already claimed by earlier rows, recorded as [`Conflict::Clash`].
    pub fn with_overrides(overrides: &HashMap<String, Vec<String>>) -> (Self, Vec<Conflict>) {
        let mut conflicts = Vec::new();
        // Per-action combo lists after overrides.
        let mut per_action: Vec<(&BindingDef, Vec<KeyCombo>)> = Vec::new();
        for def in BINDING_TABLE.iter().filter(|d| d.action.is_some()) {
            let combos = match overrides.get(def.id) {
                None => def
                    .defaults
                    .iter()
                    .map(|(spec, require, forbid)| {
                        let mut c = parse_combo(spec).expect("default spec parses");
                        c.require = *require;
                        c.forbid = *forbid;
                        c
                    })
                    .collect(),
                Some(specs) => {
                    let mut valid = Vec::new();
                    for key in specs {
                        match parse_combo(key) {
                            Some(c) => valid.push(c),
                            None => conflicts.push(Conflict::BadKey {
                                id: def.id.to_string(),
                                key: key.clone(),
                            }),
                        }
                    }
                    if valid.is_empty() {
                        // Nothing usable: keep defaults rather than unbinding.
                        def.defaults
                            .iter()
                            .map(|(spec, require, forbid)| {
                                let mut c = parse_combo(spec).expect("default spec parses");
                                c.require = *require;
                                c.forbid = *forbid;
                                c
                            })
                            .collect()
                    } else {
                        valid
                    }
                }
            };
            per_action.push((def, combos));
        }
        for id in overrides.keys() {
            if !BINDING_TABLE.iter().any(|d| d.id == id) {
                conflicts.push(Conflict::UnknownId(id.clone()));
            }
        }
        // Flatten in table order, recording later-row clashes.
        let mut map = Vec::new();
        let mut claimed: HashMap<KeyCombo, &'static str> = HashMap::new();
        for (def, combos) in &per_action {
            for combo in combos {
                if let Some(owner) = claimed.get(combo) {
                    conflicts.push(Conflict::Clash {
                        combo: combo.to_string(),
                        kept: owner,
                        dropped: def.id,
                    });
                    continue;
                }
                claimed.insert(*combo, def.id);
                map.push((*combo, def.action.expect("filtered")));
            }
        }
        (Self { map }, conflicts)
    }

    /// First table-order match wins. Returns `None` for unbound keys.
    pub fn resolve(&self, key: &KeyEvent) -> Option<GlobalAction> {
        self.map
            .iter()
            .find(|(combo, _)| combo.matches(key))
            .map(|(_, action)| *action)
    }

    /// Rows for the help overlay: current labels for matched actions plus
    /// the documentary rows. `overridden` marks user-rebound actions.
    pub fn help_rows(&self, overridden: &[String]) -> Vec<(String, String, bool)> {
        let mut rows = Vec::new();
        // Rebuild per-action combos from the resolved map (post-clash).
        for def in BINDING_TABLE {
            let label = match def.action {
                Some(_) => {
                    let combos: Vec<KeyCombo> = self
                        .map
                        .iter()
                        .filter(|(_, a)| Some(*a) == def.action)
                        .map(|(c, _)| *c)
                        .collect();
                    if combos.is_empty() {
                        continue; // fully clashed away; not shown as available
                    }
                    combos
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                }
                // Documentary rows show their static defaults.
                None => {
                    let combos: Vec<KeyCombo> = def
                        .defaults
                        .iter()
                        .filter_map(|(spec, _, _)| parse_combo(spec))
                        .collect();
                    doc_label(def.id, &combos)
                }
            };
            rows.push((
                label,
                def.description.to_string(),
                overridden.iter().any(|id| id == def.id),
            ));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn defaults_have_no_internal_clashes() {
        let (_, conflicts) = KeyBindings::with_overrides(&HashMap::new());
        assert!(conflicts.is_empty(), "{conflicts:?}");
    }

    #[test]
    fn defaults_reproduce_historical_literals() {
        let b = KeyBindings::defaults();
        // `?` matches regardless of modifiers (historical: code-only check).
        assert_eq!(
            b.resolve(&key(KeyCode::Char('?'), KeyModifiers::empty())),
            Some(GlobalAction::ToggleHelp)
        );
        assert_eq!(
            b.resolve(&key(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Some(GlobalAction::ToggleHelp)
        );
        // Ctrl+E / Ctrl+C / Ctrl+P / Ctrl+T via contains-semantics.
        for (c, a) in [
            ('e', GlobalAction::ToggleMouseCapture),
            ('c', GlobalAction::CancelOrExit),
            ('p', GlobalAction::CommandPalette),
            ('t', GlobalAction::CycleTheme),
        ] {
            assert_eq!(
                b.resolve(&key(KeyCode::Char(c), KeyModifiers::CONTROL)),
                Some(a)
            );
            // Bare letter must NOT match.
            assert_eq!(
                b.resolve(&key(KeyCode::Char(c), KeyModifiers::empty())),
                None
            );
        }
        // Tab accepted unless Ctrl held.
        assert_eq!(
            b.resolve(&key(KeyCode::Tab, KeyModifiers::empty())),
            Some(GlobalAction::ToggleChatPage)
        );
        assert_eq!(b.resolve(&key(KeyCode::Tab, KeyModifiers::CONTROL)), None);
        // g/s match regardless of modifiers (historical code-only checks).
        assert_eq!(
            b.resolve(&key(KeyCode::Char('g'), KeyModifiers::empty())),
            Some(GlobalAction::GotoFleet)
        );
        assert_eq!(
            b.resolve(&key(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            Some(GlobalAction::GotoSession)
        );
        assert_eq!(b.resolve(&key(KeyCode::Enter, KeyModifiers::empty())), None);
    }

    #[test]
    fn parse_combo_shapes() {
        let c = parse_combo("ctrl+shift+p").unwrap();
        assert_eq!(c.code, KeyCode::Char('p'));
        assert!(
            c.require
                .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
        assert_eq!(parse_combo("Tab").unwrap().code, KeyCode::Tab);
        assert_eq!(parse_combo("ESC").unwrap().code, KeyCode::Esc);
        assert_eq!(parse_combo("f12").unwrap().code, KeyCode::F(12));
        assert!(parse_combo("ctrl+f13").is_none());
        assert!(parse_combo("super+x").is_none());
        assert!(parse_combo("").is_none());
        assert!(parse_combo("ctrl+").is_none());
    }

    #[test]
    fn overrides_replace_and_report_clashes() {
        let mut ov = HashMap::new();
        ov.insert(
            "command_palette".to_string(),
            vec!["ctrl+k".to_string(), "bogus-key".to_string()],
        );
        ov.insert("cycle_theme".to_string(), vec!["ctrl+k".to_string()]);
        ov.insert("nope".to_string(), vec!["ctrl+z".to_string()]);
        let (b, conflicts) = KeyBindings::with_overrides(&ov);
        // Palette rebound; Ctrl+P no longer resolves.
        assert_eq!(
            b.resolve(&key(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            Some(GlobalAction::CommandPalette)
        );
        assert_eq!(
            b.resolve(&key(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            None
        );
        // Theme lost the clash (table order: palette first), and its override
        // replaced the default wholesale — so Ctrl+T is unbound too.
        assert_eq!(
            b.resolve(&key(KeyCode::Char('t'), KeyModifiers::CONTROL)),
            None
        );
        assert!(conflicts.contains(&Conflict::Clash {
            combo: "Ctrl+K".to_string(),
            kept: "command_palette",
            dropped: "cycle_theme",
        }));
        assert!(conflicts.contains(&Conflict::BadKey {
            id: "command_palette".to_string(),
            key: "bogus-key".to_string(),
        }));
        assert!(conflicts.contains(&Conflict::UnknownId("nope".to_string())));
    }

    #[test]
    fn all_bad_override_keys_keep_defaults() {
        let mut ov = HashMap::new();
        ov.insert("goto_fleet".to_string(), vec!["???".to_string()]);
        let (b, conflicts) = KeyBindings::with_overrides(&ov);
        assert_eq!(
            b.resolve(&key(KeyCode::Char('g'), KeyModifiers::empty())),
            Some(GlobalAction::GotoFleet)
        );
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn help_rows_cover_table() {
        let (b, _) = KeyBindings::with_overrides(&HashMap::new());
        let rows = b.help_rows(&[]);
        assert_eq!(rows.len(), BINDING_TABLE.len());
        assert!(rows.iter().any(|(l, d, _)| l == "?" && d.contains("help")));
        assert!(rows.iter().any(|(l, _, _)| l == "↑ / ↓"));
    }
}
