//! The second layer: hard rules, asked before the model is.
//!
//! ## Why this layer exists at all
//!
//! §8's ordering is **static deny → hooks → classifier**, and the reason it
//! insists on that order is this file's whole reason to exist: *"Hooks run
//! BEFORE classifier, so anything allow/deny-listed in settings.json is hard
//! rule; classifier only sees residual."*
//!
//! If a model can overrule a rule the user wrote in a file, the model is not a
//! second layer — it is a hole in the first. An attacker who can get a model to
//! agree has removed a guarantee somebody configured on purpose, and no test
//! anywhere else in this repository would notice.
//!
//! So the property to hold is not "the hook layer denies things". It is
//! **"the classifier is never consulted once the hook layer has decided."**
//! `the_classifier_is_not_asked_when_a_hook_decides` is the test; a hook layer
//! that is right *most* of the time is not this layer.
//!
//! ## No new rule format
//!
//! The rules come from `permissions::PermissionConfig`, which is what
//! `niki.toml` already deserialises into and what `PermissionChecker` already
//! enforces. A second, parallel rule format would be a second thing to
//! configure, a second thing to get wrong, and a second answer to "why was this
//! allowed" — so this layer reads the rules the product already has.
//!
//! ## `Ask` is not a decision
//!
//! A rule that says `Ask` is **not** a hook decision, so it falls through to
//! the classifier. Mapping `Ask` to "allow" because the user wrote it in a
//! file would silently disable the layer for exactly the commands someone took
//! the trouble to write a rule about.

use crate::permissions::{Permission, PermissionConfig};

use super::classifier::{ActionClassifier, Decision, Escalation, Refused, Tally};
use super::transcript::ClassifierView;

/// What the hook layer decided about an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookVerdict {
    /// A configured rule allows it. **Final** — the classifier is not asked.
    Allow,
    /// A configured rule denies it. **Final** — the classifier is not asked.
    Deny { reason: String },
    /// No rule spoke. The classifier decides this one.
    Undecided,
}

/// The hard rules, read from the configuration the product already has.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    /// `(pattern, decision)`, in the order the config lists them.
    rules: Vec<(String, HookVerdict)>,
}

impl Hooks {
    /// Build from `niki.toml`'s `[permissions.rules]`.
    ///
    /// A rule with no pattern is skipped: it matches every action, and a hook
    /// layer that fires on everything decides nothing.
    pub fn from_config(config: &PermissionConfig) -> Self {
        let mut rules: Vec<(String, HookVerdict)> = Vec::new();
        // Sorted so the verdict does not depend on `HashMap` iteration order.
        // Two runs with the same config must reach the same decision, or the
        // layer is a source of irreproducibility rather than of safety.
        let mut entries: Vec<(&String, &crate::permissions::PermissionRule)> =
            config.rules.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (_, rule) in entries {
            let Some(pattern) = rule.pattern.as_ref().filter(|p| !p.is_empty()) else {
                continue;
            };
            let verdict = match rule.permission {
                Permission::Allow => HookVerdict::Allow,
                Permission::Deny => HookVerdict::Deny {
                    reason: format!("denied by the rule matching `{pattern}`"),
                },
                Permission::Ask => continue,
            };
            rules.push((pattern.clone(), verdict));
        }
        Self { rules }
    }

    /// The rules, for a user who wants to see what is hard-wired.
    pub fn rules(&self) -> &[(String, HookVerdict)] {
        &self.rules
    }

    /// Decide one action. `arguments` is the rendered argument text, which is
    /// what the existing `PermissionChecker::check_command` matches against, so
    /// one user rule means the same thing in both places.
    pub fn decide(&self, name: &str, arguments: &str) -> HookVerdict {
        for (pattern, verdict) in &self.rules {
            if arguments.contains(pattern.as_str()) || name == pattern {
                return verdict.clone();
            }
        }
        HookVerdict::Undecided
    }
}

/// The action under consideration, taken from the view.
///
/// The last tool call in the view is the one the model has just asked for; the
/// ones before it are already history.
fn pending(view: &ClassifierView) -> Option<(String, String)> {
    view.visible.iter().rev().find_map(|v| match v {
        super::transcript::Visible::ToolCall { name, arguments } => {
            Some((name.clone(), arguments.clone()))
        }
        _ => None,
    })
}

/// Run the chain in §8's order: hooks first, and the classifier only for what
/// the hooks left undecided.
///
/// Both terminal hook verdicts **skip the classifier entirely** and do not move
/// the denial tally: a hard rule is not the model saying no a third time, and
/// counting it as one would make the escalation limits fire on rules the user
/// wrote themselves.
pub async fn adjudicate(
    hooks: &Hooks,
    classifier: &dyn ActionClassifier,
    view: &ClassifierView,
    tally: &mut Tally,
    limits: Escalation,
) -> Decision {
    if let Some((name, arguments)) = pending(view) {
        match hooks.decide(&name, &arguments) {
            HookVerdict::Allow => return Decision::Allow,
            HookVerdict::Deny { reason } => {
                return Decision::Deny(Refused { reason, stop: None });
            }
            HookVerdict::Undecided => {}
        }
    }
    super::classifier::gate(classifier, view, tally, limits).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PermissionRule;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn config(rules: &[(&str, Permission)]) -> PermissionConfig {
        let mut map = HashMap::new();
        for (pattern, permission) in rules {
            map.insert(
                pattern.to_string(),
                PermissionRule {
                    permission: *permission,
                    pattern: Some(pattern.to_string()),
                },
            );
        }
        PermissionConfig {
            rules: map,
            ..Default::default()
        }
    }

    /// A classifier that always allows, and counts how often it was asked.
    /// If it is asked at all when a hook has already decided, the count moves
    /// and the layer has a hole in it.
    struct CountingAllow {
        asked: AtomicUsize,
    }

    impl ActionClassifier for CountingAllow {
        fn classify<'a>(
            &'a self,
            _view: &'a ClassifierView,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = super::super::classifier::Verdict> + Send + 'a>,
        > {
            Box::pin(async move {
                self.asked.fetch_add(1, Ordering::SeqCst);
                super::super::classifier::Verdict::Allow
            })
        }
    }

    struct NeverApproving;

    impl ActionClassifier for NeverApproving {
        fn classify<'a>(
            &'a self,
            _view: &'a ClassifierView,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = super::super::classifier::Verdict> + Send + 'a>,
        > {
            Box::pin(async {
                super::super::classifier::Verdict::Deny {
                    reason: "the model said no".into(),
                }
            })
        }
    }

    fn view_with_command(command: &str) -> ClassifierView {
        ClassifierView {
            visible: vec![
                super::super::transcript::Visible::User("deploy it".into()),
                super::super::transcript::Visible::ToolCall {
                    name: "bash".into(),
                    arguments: format!("{{\"command\":\"{command}\"}}"),
                },
            ],
            omitted_tool_results: 0,
            omitted_assistant_chars: 0,
        }
    }

    /// The whole reason this layer exists, as a test rather than a claim.
    #[tokio::test]
    async fn the_classifier_is_not_asked_when_a_hook_decides() {
        let hooks = Hooks::from_config(&config(&[("rm -rf /", Permission::Deny)]));
        let model = CountingAllow {
            asked: AtomicUsize::new(0),
        };
        let mut tally = Tally::default();

        let denied = adjudicate(
            &hooks,
            &model,
            &view_with_command("rm -rf /tmp/x"),
            &mut tally,
            Escalation::default(),
        )
        .await;
        assert!(matches!(denied, Decision::Deny(_)), "a hard rule must deny");
        assert_eq!(
            model.asked.load(Ordering::SeqCst),
            0,
            "a hard rule is final: a model that can overrule a configured rule \
             is a hole in the first layer, not a second one"
        );

        let allowed = adjudicate(
            &hooks,
            &model,
            &view_with_command("cargo test --all"),
            &mut tally,
            Escalation::default(),
        )
        .await;
        assert_eq!(
            model.asked.load(Ordering::SeqCst),
            1,
            "an unlisted command is the residual, and the residual is the \
             classifier's job"
        );
        assert!(matches!(allowed, Decision::Allow));
    }

    /// An allow rule is final in the other direction too — otherwise a rule
    /// somebody wrote to stop asking about a common command would still cost a
    /// model call every single time it runs.
    #[tokio::test]
    async fn an_allow_rule_also_skips_the_classifier() {
        let hooks = Hooks::from_config(&config(&[("cargo test", Permission::Allow)]));
        let model = NeverApproving;
        let mut tally = Tally::default();
        let decision = adjudicate(
            &hooks,
            &model,
            &view_with_command("cargo test --all"),
            &mut tally,
            Escalation::default(),
        )
        .await;
        assert!(
            matches!(decision, Decision::Allow),
            "an allow-listed command must not be re-litigated by a model"
        );
        assert_eq!(tally.stop_reason(Escalation::default()), None);
    }

    /// `Ask` is the user saying *I want to decide this myself, and I have not*.
    /// Turning it into an allow would silently switch off the layer for
    /// precisely the commands someone took the trouble to write a rule about.
    #[tokio::test]
    async fn an_ask_rule_is_not_a_decision() {
        let hooks = Hooks::from_config(&config(&[("deploy", Permission::Ask)]));
        assert_eq!(
            hooks.decide("bash", "{\"command\":\"deploy now\"}"),
            HookVerdict::Undecided
        );
        assert!(
            hooks.rules().is_empty(),
            "an Ask rule is the residual, so it is not a hook at all"
        );
    }

    /// Two rules can match one action with opposite verdicts, so *which one
    /// wins* has to be a stated rule rather than an accident of iteration.
    /// The stated rule is: the lexicographically first pattern wins, because
    /// the rules are sorted on the way in.
    ///
    /// The sort itself cannot be tested by building the same map twice and
    /// comparing — Rust's `HashMap` order is stable within a process, so that
    /// comparison passes whether or not the sort exists. It can only be tested
    /// by asserting the outcome the sort produces, and that is what this does.
    #[test]
    fn the_first_pattern_wins_not_the_first_rule_built() {
        let hooks = Hooks::from_config(&config(&[
            ("aaa", Permission::Allow),
            ("zzz", Permission::Deny),
        ]));
        // Both patterns are in the command.
        assert_eq!(
            hooks.decide("bash", "run aaa then zzz"),
            HookVerdict::Allow,
            "aaa sorts first, so its verdict is the one that applies"
        );

        let names: Vec<&str> = hooks.rules().iter().map(|(p, _)| p.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(
            names, sorted,
            "rules are stored in pattern order: {names:?}"
        );
    }

    /// A rule with no pattern matches everything. A hook layer that decides
    /// every action has decided nothing, and worse, it would do it while
    /// looking configured.
    #[test]
    fn a_patternless_rule_is_not_a_rule() {
        let mut map = HashMap::new();
        map.insert(
            "everything".to_string(),
            PermissionRule {
                permission: Permission::Deny,
                pattern: None,
            },
        );
        let hooks = Hooks::from_config(&PermissionConfig {
            rules: map,
            ..Default::default()
        });
        assert_eq!(hooks.decide("bash", "cargo test"), HookVerdict::Undecided);
        assert!(hooks.rules().is_empty());
    }

    /// A denial tally counts what the *model* refused. A rule the user wrote
    /// denying twenty commands in a row must not trip the escalation limits and
    /// fail the run — that would blame the model for the user's own policy.
    #[tokio::test]
    async fn a_hard_denial_does_not_charge_the_models_tally() {
        let hooks = Hooks::from_config(&config(&[("forbidden", Permission::Deny)]));
        let model = CountingAllow {
            asked: AtomicUsize::new(0),
        };
        let mut tally = Tally::default();
        for _ in 0..25 {
            let d = adjudicate(
                &hooks,
                &model,
                &view_with_command("forbidden thing"),
                &mut tally,
                Escalation::default(),
            )
            .await;
            assert!(matches!(d, Decision::Deny(_)));
        }
        assert_eq!(
            model.asked.load(Ordering::SeqCst),
            0,
            "and the model was never asked about any of them"
        );
        assert_eq!(
            tally.stop_reason(Escalation::default()),
            None,
            "a user's own rules are not the model refusing twenty times"
        );
    }

    /// The decision is made **from the call inside the view**, end to end.
    ///
    /// This drives `adjudicate` rather than calling `pending` directly, because
    /// a test that reads the helper it is meant to cover cannot fail when the
    /// caller stops calling it — which is the exact sabotage that would leave
    /// the hooks deciding nothing while every other test stayed green.
    #[tokio::test]
    async fn the_decision_comes_from_the_call_in_the_view() {
        let hooks = Hooks::from_config(&config(&[("rm -rf", Permission::Deny)]));
        let model = CountingAllow {
            asked: AtomicUsize::new(0),
        };
        let mut tally = Tally::default();
        let decision = adjudicate(
            &hooks,
            &model,
            &view_with_command("rm -rf /tmp/x"),
            &mut tally,
            Escalation::default(),
        )
        .await;
        let Decision::Deny(refused) = decision else {
            panic!("the call in the view is a denied one");
        };
        assert!(
            refused.reason.contains("rm -rf"),
            "and the reason names the rule that decided it, not something else: {}",
            refused.reason
        );
        assert_eq!(
            model.asked.load(Ordering::SeqCst),
            0,
            "the hooks read the view; nobody asked the model"
        );
    }
}
