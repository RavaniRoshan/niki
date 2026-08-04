mod common;

use niki::config::DockerConfig;
use niki::sandbox::docker::DockerSandbox;

#[test]
fn parse_memory_limit_handles_gb() {
    assert_eq!(DockerSandbox::parse_memory_limit("2g"), 2 * 1024 * 1024 * 1024);
    assert_eq!(DockerSandbox::parse_memory_limit("4gb"), 4 * 1024 * 1024 * 1024);
}

#[test]
fn parse_memory_limit_handles_mb() {
    assert_eq!(DockerSandbox::parse_memory_limit("512m"), 512 * 1024 * 1024);
    assert_eq!(DockerSandbox::parse_memory_limit("100mb"), 100 * 1024 * 1024);
}

#[test]
fn parse_memory_limit_handles_kb() {
    assert_eq!(DockerSandbox::parse_memory_limit("1k"), 1024);
    assert_eq!(DockerSandbox::parse_memory_limit("512kb"), 512 * 1024);
}

#[test]
fn parse_memory_limit_handles_plain_bytes() {
    assert_eq!(DockerSandbox::parse_memory_limit("1024"), 1024);
}

#[test]
fn parse_memory_limit_returns_zero_for_invalid() {
    assert_eq!(DockerSandbox::parse_memory_limit("invalid"), 0);
    assert_eq!(DockerSandbox::parse_memory_limit(""), 0);
}

#[test]
fn parse_memory_limit_is_case_insensitive() {
    assert_eq!(DockerSandbox::parse_memory_limit("2G"), 2 * 1024 * 1024 * 1024);
    assert_eq!(DockerSandbox::parse_memory_limit("512MB"), 512 * 1024 * 1024);
}

#[test]
fn docker_config_default_has_resource_limits() {
    let config = DockerConfig::default();
    assert_eq!(config.memory_limit, "2g");
    assert!((config.cpu_limit - 2.0).abs() < f32::EPSILON);
    assert!(!config.base_image.is_empty());
}

#[test]
fn docker_config_default_has_sandbox_image() {
    let config = DockerConfig::default();
    assert!(
        config.base_image.contains("sandbox"),
        "default base image should contain 'sandbox': {}",
        config.base_image
    );
}
