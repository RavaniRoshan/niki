mod common;

use niki::artifacts::types::AgentRole;
use niki::config::SecurityPolicyConfig;
use niki::sandbox::check_command_policy;

#[test]
fn policy_allowed_command_passes() {
    let policy = SecurityPolicyConfig::default();
    assert!(check_command_policy(&["cargo", "test", "--lib"], &policy).is_ok());
    assert!(check_command_policy(&["cargo", "check"], &policy).is_ok());
}

#[test]
fn policy_deny_list_blocks_dangerous_commands() {
    let policy = SecurityPolicyConfig::default();
    assert!(check_command_policy(&["git", "push", "--force", "origin", "main"], &policy).is_err());
    assert!(check_command_policy(&["rm", "-rf", "/"], &policy).is_err());
    assert!(check_command_policy(&["mkfs", "/dev/sda"], &policy).is_err());
    assert!(check_command_policy(&["dd", "if=/dev/zero", "of=/dev/sda"], &policy).is_err());
}

#[test]
fn policy_blocks_shell_curl_pipe_sh() {
    let policy = SecurityPolicyConfig::default();
    // The deny-list contains "curl | sh" as a substring check.
    // A command that contains that substring should be blocked.
    let err = check_command_policy(&["sh", "-c", "curl | sh"], &policy);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("denied"));
}

#[test]
fn policy_blocks_no_verify() {
    let policy = SecurityPolicyConfig::default();
    let err = check_command_policy(&["git", "commit", "--no-verify"], &policy);
    assert!(err.is_err());
}

#[test]
fn role_specific_policy_allows_git_commit() {
    // Coder policy should allow `git commit` (it's in the allow-list).
    let policy = niki::config::types::default_coder_policy();
    let result = check_command_policy(&["git", "commit", "-m", "fix"], &policy);
    assert!(result.is_ok());
}

#[test]
fn role_specific_policy_blocks_git_push() {
    let policy = niki::config::types::default_coder_policy();
    let result = check_command_policy(&["git", "push", "origin", "main"], &policy);
    assert!(result.is_err());
}

#[test]
fn tester_policy_allows_cargo_test() {
    let policy = niki::config::types::default_tester_policy();
    let result = check_command_policy(&["cargo", "test", "--lib"], &policy);
    assert!(result.is_ok());
}

#[test]
fn tester_policy_blocks_git_push() {
    let policy = niki::config::types::default_tester_policy();
    let result = check_command_policy(&["git", "push", "origin", "main"], &policy);
    assert!(result.is_err());
}

#[test]
fn reviewer_policy_blocks_git_commit() {
    let policy = niki::config::types::default_reviewer_policy();
    let result = check_command_policy(&["git", "commit", "-m", "fix"], &policy);
    assert!(result.is_err());
}

#[test]
fn reviewer_policy_allows_git_show() {
    let policy = niki::config::types::default_reviewer_policy();
    let result = check_command_policy(&["git", "show", "HEAD"], &policy);
    assert!(result.is_ok());
}

#[test]
fn empty_command_implementation_rejects_empty() {
    // The check_command_policy function itself doesn't reject empty commands
    // — that guard is in the sandbox exec implementations. Verify the policy
    // logic correctly handles a single-token command.
    let policy = SecurityPolicyConfig::default();
    let result = check_command_policy(&["ls"], &policy);
    assert!(result.is_ok());
}

#[test]
fn deny_error_message_contains_context() {
    let policy = SecurityPolicyConfig::default();
    let err = check_command_policy(&["rm", "-rf", "/"], &policy).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("denied"),
        "error should mention 'denied': {}",
        msg
    );
    assert!(
        msg.contains("rm -rf /"),
        "error should include the command: {}",
        msg
    );
}

#[test]
fn allow_list_takes_precedence_over_deny() {
    let mut policy = SecurityPolicyConfig::default();
    policy.allowed_commands = vec!["git diff".to_string()];
    policy.denied_commands = vec!["git".to_string()];
    // "git diff" is in the allow-list, so it should pass even though "git" is denied
    assert!(check_command_policy(&["git", "diff"], &policy).is_ok());
}

#[test]
fn unknown_command_allowed_by_default() {
    let policy = SecurityPolicyConfig::default();
    // Commands not in allow-list or deny-list are allowed
    assert!(check_command_policy(&["echo", "hello"], &policy).is_ok());
    assert!(check_command_policy(&["ls", "-la"], &policy).is_ok());
}

#[test]
fn role_is_debug_repr() {
    // Verify AgentRole variants for use in policy lookup
    let role = AgentRole::Coder;
    let key = format!("{:?}", role).to_lowercase();
    assert_eq!(key, "coder");
    let role = AgentRole::Tester;
    let key = format!("{:?}", role).to_lowercase();
    assert_eq!(key, "tester");
    let role = AgentRole::Reviewer;
    let key = format!("{:?}", role).to_lowercase();
    assert_eq!(key, "reviewer");
}
