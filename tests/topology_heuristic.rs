//! The topology heuristic was answering a question it had no data for.
//!
//! `Auto` looked only at the task: a low-complexity task collapsed to the
//! single-agent fast path, whatever was running it. The compute-matched
//! ablation of single vs multi-agent topologies (260 configurations, SWE-bench
//! Verified included, arXiv:2512.08296) says that is backwards for a weak
//! model — below a 45% single-agent baseline a multi-agent pipeline is worth
//! about +22 points, and above 50% it costs about 5.
//!
//! NIKI's pitch is a *small local model*: the README's zero-setup path is
//! `ollama pull qwen2.5-coder:3b`. So the old heuristic gave precisely the
//! structure-hungry model the structure-hungry path's opposite, and a live run
//! against that model died at the Coder.

use niki::artifacts::types::{Complexity, TaskSpec};
use niki::config::capability::{ModelCapability, SATURATION, save};
use niki::config::types::NikiConfig;
use niki::orchestrator::pipeline::{select_topology, topology_reason};
use std::path::Path;

fn config_in(dir: &Path) -> NikiConfig {
    NikiConfig {
        project_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

fn spec(complexity: Complexity) -> TaskSpec {
    TaskSpec {
        summary: "s".into(),
        approach: "a".into(),
        files_to_modify: vec![],
        acceptance_criteria: vec![],
        constraints: vec![],
        estimated_complexity: complexity,
        uncertainties: None,
    }
}

#[test]
fn an_unmeasured_model_gets_the_multi_agent_chain() {
    // The default for every user who has not run `niki doctor --measure`. The
    // asymmetry is deliberate: structure is worth ~22pp to a weak model and
    // costs ~5pp to a strong one, so guessing "weak" is the cheap mistake.
    let dir = tempfile::tempdir().expect("tempdir");
    let c = config_in(dir.path());
    assert_eq!(c.pipeline.topology, niki::config::types::TopologyMode::Auto);
    assert_eq!(
        select_topology(&spec(Complexity::Low), &c),
        niki::config::types::TopologyMode::MultiAgent,
        "an unmeasured model must not be assumed strong — that is how a 3B model ended up on \
         the single-agent path"
    );
}

#[test]
fn a_measured_weak_model_still_gets_the_chain() {
    let dir = tempfile::tempdir().expect("tempdir");
    save(
        dir.path(),
        ModelCapability::Measured {
            passed: 1,
            total: 4,
        },
    )
    .expect("save");
    let c = config_in(dir.path());
    assert_eq!(
        select_topology(&spec(Complexity::Low), &c),
        niki::config::types::TopologyMode::MultiAgent
    );
}

#[test]
fn a_measured_strong_model_gets_the_fast_path_for_a_simple_task() {
    // The other half, and the reason the change is not simply "always multi".
    // For a strong model the old collapse is right, and the paper says so.
    let dir = tempfile::tempdir().expect("tempdir");
    save(
        dir.path(),
        ModelCapability::Measured {
            passed: 9,
            total: 10,
        },
    )
    .expect("save");
    let c = config_in(dir.path());
    assert_eq!(
        select_topology(&spec(Complexity::Low), &c),
        niki::config::types::TopologyMode::SingleAgent,
        "a strong model on a simple task should still take the fast path"
    );
    // …and a hard task still gets the chain regardless.
    assert_eq!(
        select_topology(&spec(Complexity::High), &c),
        niki::config::types::TopologyMode::MultiAgent
    );
}

#[test]
fn an_explicit_topology_is_never_overridden() {
    // The measurement is advice, not policy. A user who pins a topology means
    // it, including when we would have chosen otherwise.
    for pinned in [
        niki::config::types::TopologyMode::SingleAgent,
        niki::config::types::TopologyMode::MultiAgent,
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        save(
            dir.path(),
            ModelCapability::Measured {
                passed: 0,
                total: 4,
            },
        )
        .expect("save");
        let mut c = config_in(dir.path());
        c.pipeline.topology = pinned;
        assert_eq!(select_topology(&spec(Complexity::Low), &c), pinned);
        assert!(
            topology_reason(&spec(Complexity::Low), &c).contains("explicit"),
            "an explicit choice must say so"
        );
    }
}

#[test]
fn the_reason_tells_the_user_which_way_the_trade_went() {
    // A silent heuristic is the thing that made this a bug. Every branch says
    // what it decided and why, in the report the user reads.
    let dir = tempfile::tempdir().expect("tempdir");
    let c = config_in(dir.path());
    let unmeasured = topology_reason(&spec(Complexity::Low), &c);
    assert!(unmeasured.contains("not measured"), "{unmeasured}");
    assert!(unmeasured.contains("doctor --measure"), "{unmeasured}");

    save(
        dir.path(),
        ModelCapability::Measured {
            passed: 1,
            total: 4,
        },
    )
    .expect("save");
    let weak = topology_reason(&spec(Complexity::Low), &c);
    assert!(weak.contains("1/4"), "{weak}");

    save(
        dir.path(),
        ModelCapability::Measured {
            passed: 9,
            total: 10,
        },
    )
    .expect("save");
    let strong = topology_reason(&spec(Complexity::Low), &c);
    assert!(strong.contains("measured strong"), "{strong}");
}

#[test]
fn the_threshold_is_the_measured_one_not_a_rounded_guess() {
    // 45% is where coordination returns saturate, and the only effect in that
    // study surviving cluster-robust inference. It is written down here so
    // changing it is a deliberate act.
    assert!((SATURATION - 0.45).abs() < f64::EPSILON);
}

#[test]
fn a_config_loaded_from_disk_knows_its_project() {
    // The heuristic reads a file, so it has to know which project.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("niki.toml"),
        "[general]\nmax_revision_rounds = 4\n",
    )
    .unwrap();
    let c = NikiConfig::load(dir.path()).expect("load");
    assert_eq!(c.project_dir_hint(), dir.path());
    assert_eq!(c.general.max_revision_rounds, 4);
}

/// A 0/N probe result must not steer the topology.
///
/// The probe asks a deliberately trivial question — "replace `old` with `new`" —
/// and a small model answers it minimally: the `edits` it was asked for and
/// nothing else. `code_diff.schema.json` requires four properties, so the answer
/// fails validation and the probe scores 0/N.
///
/// That is a fact about the probe, not about the model. The same model, given
/// the real Coder prompt with a specification and the file contents, emits the
/// whole artifact and completes four-agent runs — which was measured on this
/// machine before the routing consequence was noticed.
///
/// Letting a measurement this confounded decide that a usable model should take
/// the slow path would be worse than having no probe at all.
#[test]
fn a_clean_zero_probe_result_falls_back_to_unknown() {
    let zero = ModelCapability::Measured {
        passed: 0,
        total: 4,
    };
    assert_eq!(
        zero.usable(),
        ModelCapability::Unknown,
        "a 0/N is confounded by a trivial ask, not a measurement of the model"
    );
    // The unknown case routes to the safe side — the multi-agent chain.
    assert!(zero.usable().benefits_from_structure());

    // Anything with a pass is a real signal and is kept.
    for real in [
        ModelCapability::Measured {
            passed: 1,
            total: 4,
        },
        ModelCapability::Measured {
            passed: 4,
            total: 4,
        },
    ] {
        assert_eq!(real.usable(), real);
    }
    // And the explanation has to say why, so the next reader is not misled by
    // a 0% that is reported as unmeasured.
    let msg = zero.explain();
    assert!(msg.contains("UNKNOWN"), "{msg}");
    assert!(
        msg.contains("trivial") || msg.contains("probe"),
        "and must name the probe as the confounded thing: {msg}"
    );
}
