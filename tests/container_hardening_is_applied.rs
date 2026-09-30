//! The container must actually be hardened.
//!
//! `tests/docker_resource_caps.rs` is *named* after container resource caps. It
//! has eight tests. Six are on `parse_memory_limit` — one string parser — and
//! two on config defaults. **None** of them checks the four settings the file
//! is named for:
//!
//! | Setting | In `src/` | In `tests/` |
//! |---|---|---|
//! | `cap_drop` | yes | **no** |
//! | `pids_limit` | yes | **no** |
//! | `network_mode` | yes | **no** |
//! | `readonly_rootfs` | yes | **no** |
//!
//! The code applies all four — `src/sandbox/docker.rs` built them inline inside
//! the container-create path, so nothing could reach them from a test. A
//! security posture asserted by a file that tests something else is not
//! asserted.
//!
//! `build_host_config` is now a plain function over the real `DockerConfig`, so
//! these assertions run on the value that is actually sent to Docker. The
//! defaults matter most: `cap_drop_all`, `network_disabled` and
//! `readonly_rootfs` must harden a *default* install, and a user who
//! reconfigures one setting must not silently lose the others.

use niki::config::types::DockerConfig;
use niki::sandbox::docker::build_host_config;

fn host(config: &DockerConfig) -> bollard::models::HostConfig {
    build_host_config(
        config,
        vec!["/work:/workspace".to_string()],
        2 << 30,
        2_000_000,
    )
}

/// **The defaults must harden.** A fresh install is the case that matters.
#[test]
fn a_default_container_is_hardened() {
    let config = DockerConfig::default();
    let h = host(&config);

    assert_eq!(
        h.cap_drop.as_deref(),
        Some(["ALL".to_string()].as_slice()),
        "the default container must drop ALL capabilities. The agent runtime \
         needs none of them, and `cap_drop_all` defaults to true"
    );
    assert_eq!(
        h.pids_limit,
        Some(config.pids_limit as i64),
        "the default must bound the process count, so a fork-bomb or runaway \
         recursion is contained"
    );
    assert!(
        config.pids_limit > 0,
        "a default pids_limit of 0 means no bound at all, and the `Some(0)` \
         branch below would never be taken"
    );
    assert_eq!(
        h.network_mode.as_deref(),
        Some("none"),
        "egress is blocked by default — `network_disabled` defaults to true — so \
         the container's network mode must be `none`"
    );
    assert_eq!(
        h.readonly_rootfs, None,
        "`readonly_rootfs` defaults to false, so the field must be unset rather \
         than asserted true"
    );
}

/// The one setting a user is expected to flip is network egress, and it must
/// flip on its own without disturbing the rest.
#[test]
fn opening_egress_does_not_unharden_anything_else() {
    let mut config = DockerConfig::default();
    assert_eq!(host(&config).network_mode.as_deref(), Some("none"));

    config.network_disabled = false;
    let opened = host(&config);
    assert_eq!(
        opened.network_mode, None,
        "`network_disabled = false` must remove the `none` network mode"
    );
    // Everything else is untouched: opening the network must not quietly grant
    // capabilities or make the rootfs writable.
    assert_eq!(
        opened.cap_drop.as_deref(),
        Some(["ALL".to_string()].as_slice()),
        "opening egress dropped the capability hardening"
    );
    assert_eq!(opened.pids_limit, Some(config.pids_limit as i64));
}

/// And `network_allowlist = ["*"]` is the documented equivalent, which the code
/// treats specially.
#[test]
fn a_wildcard_allowlist_opens_egress_too() {
    let config = DockerConfig {
        network_allowlist: vec!["*".to_string()],
        ..DockerConfig::default()
    };
    let h = host(&config);
    assert_eq!(
        h.network_mode, None,
        "`network_allowlist = [\"*\"]` is documented as equivalent to \\
         `network_disabled = false`, and must behave the same way"
    );
    assert_eq!(
        h.cap_drop.as_deref(),
        Some(["ALL".to_string()].as_slice()),
        "and must not cost the capability hardening"
    );
}

/// Turning a hardening setting **off** must actually turn it off — otherwise
/// the config keys are decoration.
#[test]
fn each_hardening_setting_can_be_turned_off() {
    // Each flipped on its own, from a fresh default, so a setting cannot be
    // masked by another.
    assert_eq!(
        host(&DockerConfig {
            cap_drop_all: false,
            ..DockerConfig::default()
        })
        .cap_drop,
        None,
        "`cap_drop_all = false` must stop dropping capabilities"
    );
    assert_eq!(
        host(&DockerConfig {
            pids_limit: 0,
            ..DockerConfig::default()
        })
        .pids_limit,
        None,
        "`pids_limit = 0` must mean no limit rather than a limit of zero, which \
         Docker would read as \"no processes can be created\""
    );
    assert_eq!(
        host(&DockerConfig {
            readonly_rootfs: true,
            ..DockerConfig::default()
        })
        .readonly_rootfs,
        Some(true),
        "`readonly_rootfs = true` must be sent to Docker"
    );
}

/// And the resource limits, which *is* what the file was named for, so the
/// original tests have a neighbour to live beside.
#[test]
fn the_resource_limits_reach_the_host_config() {
    let config = DockerConfig::default();
    let h = host(&config);
    assert_eq!(h.memory, Some(2 << 30), "memory must be sent as bytes");
    assert_eq!(h.nano_cpus, Some(2_000_000), "cpu must be sent as NanoCPUs");
    assert_eq!(
        h.binds.as_deref(),
        Some(["/work:/workspace".to_string()].as_slice()),
        "the workspace bind must survive the extraction"
    );
}

/// No container may be privileged, whatever the config says.
#[test]
fn a_container_is_never_privileged() {
    for (label, config) in [
        ("default", DockerConfig::default()),
        (
            "hardening off",
            DockerConfig {
                cap_drop_all: false,
                pids_limit: 0,
                network_disabled: false,
                readonly_rootfs: false,
                ..DockerConfig::default()
            },
        ),
    ] {
        let h = host(&config);
        assert!(
            !h.privileged.unwrap_or(false),
            "{label}: the container must never be privileged. Nothing in the \\
             config surface can grant it, and this is what pins that"
        );
        assert!(
            h.cap_add.is_none(),
            "{label}: no capability may be added back; the point of \
             `cap_drop_all` is that the agent runtime needs none"
        );
    }
}
