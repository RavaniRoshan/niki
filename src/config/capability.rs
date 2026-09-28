//! How capable is the configured model, measured rather than guessed.
//!
//! This exists because the topology heuristic was answering a question it had
//! no data for. `Auto` looked only at the *task* — a low-complexity task
//! collapsed to a single agent — and never at the *model*.
//!
//! The compute-matched ablation of single vs multi-agent topologies (260
//! configurations, SWE-bench Verified included, arXiv:2512.08296) makes the
//! shape of the mistake precise:
//!
//! | single-agent baseline | effect of a multi-agent pipeline |
//! |---|---|
//! | below 45% | **+21.7pp** |
//! | above 50%  | **−5.0pp** |
//!
//! So the same heuristic is strongly right for a frontier model and strongly
//! wrong for a small local one — and NIKI's pitch is a small local one. A user
//! who follows the README's zero-setup path gets a 3B model *and* the collapse
//! that hurts 3B models most, which is precisely the combination that made the
//! product unusable.
//!
//! Nothing here infers capability from a model *name*. A name is a guess that
//! rots; `niki doctor --measure` runs a cheap probe and records what it saw.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The point above which extra structure stops paying.
///
/// 45% is where coordination returns saturate in the ablation, and it is the
/// single effect there that survives cluster-robust inference (p=0.004). It is a
/// measured constant, not a taste.
pub const SATURATION: f64 = 0.45;

/// What we know about the configured model's ability to emit conformant
/// artifacts, which is the capability the pipeline actually depends on.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ModelCapability {
    #[default]
    /// Never measured. The default for every user who has not run
    /// `niki doctor --measure`.
    Unknown,
    /// Measured: `passed` probes out of `total` produced a valid artifact.
    Measured { passed: u32, total: u32 },
    /// A probe was attempted and the model could not be reached at all.
    Unreachable,
}

impl ModelCapability {
    /// The pass rate, or `None` when it was never measured.
    pub fn rate(&self) -> Option<f64> {
        match self {
            ModelCapability::Measured { passed, total } if *total > 0 => {
                Some(*passed as f64 / *total as f64)
            }
            _ => None,
        }
    }

    /// Whether a multi-agent pipeline is expected to help this model.
    ///
    /// `Unknown` returns `true`, and that is a deliberate asymmetry rather than
    /// a missing branch. The evidence is a trade: structure buys ~22 points for
    /// a weak model and costs ~5 for a strong one, so running without it when the
    /// model is weak is the expensive mistake and running with it when the model
    /// is strong is the cheap one. An unmeasured model should be treated as
    /// weak, because "weak" is the case with the bad downside — a run that dies
    /// at the Coder — rather than the case with merely slower output.
    pub fn benefits_from_structure(&self) -> bool {
        match self.rate() {
            Some(rate) => rate < SATURATION,
            None => true,
        }
    }

    /// One line for `niki doctor` and for the run report.
    pub fn explain(&self) -> String {
        match self {
            ModelCapability::Unknown => {
                "not measured — assuming a multi-agent pipeline, which is the safer side of the \
                 trade (it is worth about +22 points to a weak model and costs about 5 to a \
                 strong one). Run `niki doctor --measure` to replace this guess with a \
                 measurement."
                    .to_string()
            }
            ModelCapability::Unreachable => {
                "could not be measured — the model did not respond. Treated as unknown.".to_string()
            }
            ModelCapability::Measured { passed, total } => {
                let rate = if *total == 0 {
                    0.0
                } else {
                    *passed as f64 / *total as f64
                };
                format!(
                    "{passed}/{total} artifact probes passed ({:.0}%); the measured threshold is \
                     {:.0}%, so {}",
                    rate * 100.0,
                    SATURATION * 100.0,
                    if self.benefits_from_structure() {
                        "a multi-agent pipeline is expected to help"
                    } else {
                        "the single-agent fast path is expected to be enough"
                    }
                )
            }
        }
    }
}

/// Where the measurement lives, relative to the project.
const STORE: &str = ".niki/model-capability.json";

/// Read the recorded measurement for a project, if there is one.
pub fn load(project_dir: &Path) -> ModelCapability {
    let path = project_dir.join(STORE);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(ModelCapability::Unknown)
}

/// Record a measurement for a project.
pub fn save(project_dir: &Path, capability: ModelCapability) -> std::io::Result<PathBuf> {
    let path = project_dir.join(STORE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&capability).unwrap_or_default(),
    )?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unmeasured_model_is_treated_as_one_that_needs_structure() {
        // The asymmetry is the point. Structure is worth ~22pp to a weak model
        // and costs ~5pp to a strong one, so guessing "weak" is the cheap
        // mistake and guessing "strong" is the expensive one.
        assert!(ModelCapability::Unknown.benefits_from_structure());
        assert!(ModelCapability::Unreachable.benefits_from_structure());
    }

    #[test]
    fn a_measured_model_crosses_over_at_the_saturation_point() {
        for (passed, total, expected) in [
            (0u32, 4u32, true), // 0%  — badly weak
            (1, 4, true),       // 25%
            (2, 5, true),       // 40% — below 45
            (5, 10, false),     // 50% — above
            (9, 10, false),     // 90%
        ] {
            let c = ModelCapability::Measured { passed, total };
            assert_eq!(
                c.benefits_from_structure(),
                expected,
                "{passed}/{total} should {}structure",
                if expected { "benefit from" } else { "not need" }
            );
        }
    }

    #[test]
    fn an_empty_measurement_is_not_a_hundred_percent() {
        // 0/0 must not divide into `NaN` and then compare false, quietly
        // choosing the fast path for a model that was never really tested.
        let c = ModelCapability::Measured {
            passed: 0,
            total: 0,
        };
        assert_eq!(c.rate(), None);
        assert!(c.benefits_from_structure());
    }

    #[test]
    fn the_explanation_names_the_threshold_and_the_next_step() {
        let unknown = ModelCapability::Unknown.explain();
        assert!(unknown.contains("not measured"), "{unknown}");
        assert!(unknown.contains("doctor --measure"), "{unknown}");

        let measured = ModelCapability::Measured {
            passed: 2,
            total: 4,
        }
        .explain();
        assert!(measured.contains("2/4"), "{measured}");
        assert!(measured.contains("45%"), "{measured}");
    }

    #[test]
    fn a_measurement_round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            load(dir.path()),
            ModelCapability::Unknown,
            "no file means unknown"
        );
        save(
            dir.path(),
            ModelCapability::Measured {
                passed: 3,
                total: 4,
            },
        )
        .expect("save");
        assert_eq!(
            load(dir.path()),
            ModelCapability::Measured {
                passed: 3,
                total: 4
            }
        );
    }
}
