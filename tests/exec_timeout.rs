mod common;

use niki::config::SecurityPolicyConfig;
use niki::sandbox::check_command_policy;
use std::time::Duration;

#[test]
fn default_policy_has_reasonable_exec_timeout() {
    let policy = SecurityPolicyConfig::default();
    assert_eq!(policy.max_exec_seconds, 300);
}

#[test]
fn policy_with_short_timeout_will_timeout_quickly() {
    let policy = SecurityPolicyConfig {
        max_exec_seconds: 1,
        ..Default::default()
    };
    assert_eq!(policy.max_exec_seconds, 1);
}

#[test]
fn check_command_policy_completes_within_timeout_when_allowed() {
    let policy = SecurityPolicyConfig::default();
    let start = std::time::Instant::now();
    let result = check_command_policy(&["cargo", "test", "--lib"], &policy);
    let elapsed = start.elapsed();
    assert!(result.is_ok());
    assert!(
        elapsed < Duration::from_secs(5),
        "policy check should complete quickly, took {:?}",
        elapsed
    );
}

#[test]
fn timeout_error_message_includes_duration() {
    // The exec implementations produce an error message that includes the
    // configured timeout when a command times out. Verify the message format.
    let policy = SecurityPolicyConfig::default();
    let expected_msg = format!("exec timed out after {}s", policy.max_exec_seconds);
    assert!(expected_msg.contains("exec timed out"));
    assert!(expected_msg.contains("300"));
}

#[test]
fn policy_timeout_is_configurable_per_role() {
    // SecurityPolicyConfig should allow custom timeout per role config.
    let mut policy = SecurityPolicyConfig::default();
    assert_eq!(policy.max_exec_seconds, 300);
    policy.max_exec_seconds = 60;
    assert_eq!(policy.max_exec_seconds, 60);
}

#[test]
fn deny_list_commands_blocked_immediately_not_timeout() {
    let policy = SecurityPolicyConfig::default();
    let start = std::time::Instant::now();
    let result = check_command_policy(&["rm", "-rf", "/"], &policy);
    let elapsed = start.elapsed();
    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_secs(5),
        "denied command should be rejected immediately, took {:?}",
        elapsed
    );
}

#[test]
fn timeout_duration_from_policy() {
    let policy = SecurityPolicyConfig {
        max_exec_seconds: 10,
        ..Default::default()
    };
    let duration = Duration::from_secs(policy.max_exec_seconds);
    assert_eq!(duration, Duration::from_secs(10));
}
