//! Modal behavior tests.

use ratatui::crossterm::event::KeyCode;
use niki::display::pages::{Modal, PageId, PageRouter};

mod helpers;
use helpers::*;

// ═══════════════════════════════════════════════════════════════════════════
// Modal creation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn confirm_modal_created_by_run_page_esc() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    match &state.modal {
        Some(Modal::Confirm { title, message }) => {
            assert!(title.contains("Quit"));
            assert!(!message.is_empty());
        }
        other => panic!("expected Confirm modal, got {other:?}"),
    }
}

#[test]
fn confirm_modal_created_by_run_page_q() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;
    press(&mut router, &mut state, key_char('q'));
    assert!(matches!(&state.modal, Some(Modal::Confirm { .. })));
}

#[test]
fn error_modal_can_be_created() {
    let mut state = test_state();
    state.modal = Some(Modal::Error {
        stage: "Coder".into(),
        message: "API timeout".into(),
        hint: "Check network".into(),
    });
    match &state.modal {
        Some(Modal::Error { stage, message, hint }) => {
            assert_eq!(stage, "Coder");
            assert_eq!(message, "API timeout");
            assert_eq!(hint, "Check network");
        }
        other => panic!("expected Error modal, got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Modal key handling (Confirm)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn confirm_modal_esc_dismisses() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Confirm {
        title: "Quit?".into(),
        message: "Are you sure?".into(),
    };
    let action = handle_modal_key(key_code(KeyCode::Esc), &modal);
    assert!(matches!(action, ModalAction::Dismiss));
}

#[test]
fn confirm_modal_enter_confirms() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Confirm {
        title: "Quit?".into(),
        message: "Are you sure?".into(),
    };
    let action = handle_modal_key(key_code(KeyCode::Enter), &modal);
    assert!(matches!(action, ModalAction::Confirm));
}

#[test]
fn confirm_modal_other_keys_are_none() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Confirm {
        title: "Quit?".into(),
        message: "Are you sure?".into(),
    };
    assert!(matches!(handle_modal_key(key_char('a'), &modal), ModalAction::None));
    assert!(matches!(handle_modal_key(key_char('r'), &modal), ModalAction::None));
    assert!(matches!(handle_modal_key(key_char('c'), &modal), ModalAction::None));
}

// ═══════════════════════════════════════════════════════════════════════════
// Modal key handling (Error)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn error_modal_esc_dismisses() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Error {
        stage: "Coder".into(),
        message: "Failed".into(),
        hint: "Retry".into(),
    };
    let action = handle_modal_key(key_code(KeyCode::Esc), &modal);
    assert!(matches!(action, ModalAction::Dismiss));
}

#[test]
fn error_modal_enter_dismisses() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Error {
        stage: "Coder".into(),
        message: "Failed".into(),
        hint: "Retry".into(),
    };
    let action = handle_modal_key(key_code(KeyCode::Enter), &modal);
    assert!(matches!(action, ModalAction::Dismiss));
}

#[test]
fn error_modal_r_retries() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Error {
        stage: "Coder".into(),
        message: "Failed".into(),
        hint: "Retry".into(),
    };
    let action = handle_modal_key(key_char('r'), &modal);
    assert!(matches!(action, ModalAction::Retry));
}

#[test]
fn error_modal_c_opens_config() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Error {
        stage: "Coder".into(),
        message: "Failed".into(),
        hint: "Retry".into(),
    };
    let action = handle_modal_key(key_char('c'), &modal);
    assert!(matches!(action, ModalAction::Config));
}

#[test]
fn error_modal_other_keys_are_none() {
    use niki::display::modal::{handle_modal_key, ModalAction};
    let modal = Modal::Error {
        stage: "Coder".into(),
        message: "Failed".into(),
        hint: "Retry".into(),
    };
    assert!(matches!(handle_modal_key(key_char('a'), &modal), ModalAction::None));
    assert!(matches!(handle_modal_key(key_char('v'), &modal), ModalAction::None));
}

// ═══════════════════════════════════════════════════════════════════════════
// Modal dismiss flow (integration with AppState)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn modal_dismiss_clears_modal_field() {
    let mut state = test_state();
    state.modal = Some(Modal::Confirm {
        title: "test".into(),
        message: "test".into(),
    });
    use niki::display::modal::{handle_modal_key, ModalAction};
    if let Some(ref modal) = state.modal {
        let action = handle_modal_key(key_code(KeyCode::Esc), modal);
        if let ModalAction::Dismiss = action {
            state.modal = None;
        }
    }
    assert!(state.modal.is_none());
}

#[test]
fn modal_config_action_navigates_to_config() {
    let mut state = test_state();
    state.modal = Some(Modal::Error {
        stage: "test".into(),
        message: "test".into(),
        hint: "test".into(),
    });
    use niki::display::modal::{handle_modal_key, ModalAction};
    let action = {
        let modal = state.modal.as_ref().unwrap();
        handle_modal_key(key_char('c'), modal)
    };
    if let ModalAction::Config = action {
        state.modal = None;
        state.current_page = PageId::Config;
    }
    assert_eq!(state.current_page, PageId::Config);
    assert!(state.modal.is_none());
}
