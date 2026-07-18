use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::{
    InitialContextApplicationEvidence, InitialContextDeliveryFidelity,
    RequiredInitialContextEvidence, RuntimeAgentChildIdentity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionContextMode {
    Full,
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionAdoptionMode {
    Suggestion,
    BlockingReview,
    FollowUpRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextContributionProvenance {
    pub authority: String,
    pub source_coordinate: String,
    pub source_revision: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InitialAgentContextContribution {
    CompactSummary {
        summary: String,
        provenance: ContextContributionProvenance,
    },
    WorkflowContext {
        schema: String,
        value: Value,
        provenance: ContextContributionProvenance,
    },
    ConstraintSet {
        schema: String,
        value: Value,
        provenance: ContextContributionProvenance,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitialAgentContextPackage {
    pub package_id: Uuid,
    pub schema_version: u32,
    pub mode: CompanionContextMode,
    pub contributions: Vec<InitialAgentContextContribution>,
    pub digest: String,
}

impl InitialAgentContextPackage {
    fn calculate_digest(
        package_id: Uuid,
        schema_version: u32,
        mode: CompanionContextMode,
        contributions: &[InitialAgentContextContribution],
    ) -> String {
        let canonical = serde_json::to_vec(&(package_id, schema_version, mode, contributions))
            .expect("typed initial context package must serialize");
        format!("sha256:{:x}", Sha256::digest(canonical))
    }

    pub fn digest_matches(&self) -> bool {
        self.digest
            == Self::calculate_digest(
                self.package_id,
                self.schema_version,
                self.mode,
                &self.contributions,
            )
    }

    pub fn required_application_evidence(&self) -> RequiredInitialContextEvidence {
        RequiredInitialContextEvidence {
            package_id: self.package_id,
            package_digest: self.digest.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitInput {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompanionRuntimePreparation {
    ForkParentHistory {
        parent_source_coordinate: String,
        through_turn_id: String,
    },
    FreshCreate {
        initial_context: InitialAgentContextPackage,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionDispatchTargetPlan {
    pub preparation: CompanionRuntimePreparation,
    pub adoption_mode: CompanionAdoptionMode,
    pub first_submit_input: SubmitInput,
    /// Business Surface facts remain a separate target input. They are not
    /// serialized into `InitialAgentContextPackage`.
    pub surface_facts: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionContextSources {
    pub parent_source_coordinate: String,
    pub through_turn_id: Option<String>,
    pub package_id: Uuid,
    pub compact_summary: Option<(String, ContextContributionProvenance)>,
    pub workflow: Option<(String, Value, ContextContributionProvenance)>,
    pub constraints: Option<(String, Value, ContextContributionProvenance)>,
    pub surface_facts: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompanionTargetPlanError {
    #[error("Full companion context requires an exact parent turn cutoff")]
    MissingParentTurnCutoff,
    #[error("{mode:?} companion context has no typed contribution")]
    MissingTypedContribution { mode: CompanionContextMode },
    #[error("initial context package digest is invalid")]
    InvalidPackageDigest,
    #[error("activation requires applied initial context evidence")]
    MissingContextEvidence,
    #[error("applied initial context evidence does not match the package")]
    ContextEvidenceMismatch,
    #[error("applied initial context evidence reports unsupported delivery fidelity")]
    UnsupportedContextFidelity,
    #[error("Full history fork evidence does not match the exact parent history request")]
    ForkHistoryEvidenceMismatch,
}

pub fn compile_companion_dispatch_target(
    mode: CompanionContextMode,
    adoption_mode: CompanionAdoptionMode,
    task: SubmitInput,
    sources: CompanionContextSources,
) -> Result<CompanionDispatchTargetPlan, CompanionTargetPlanError> {
    let preparation = match mode {
        CompanionContextMode::Full => CompanionRuntimePreparation::ForkParentHistory {
            parent_source_coordinate: sources.parent_source_coordinate,
            through_turn_id: sources
                .through_turn_id
                .ok_or(CompanionTargetPlanError::MissingParentTurnCutoff)?,
        },
        CompanionContextMode::Compact
        | CompanionContextMode::WorkflowOnly
        | CompanionContextMode::ConstraintsOnly => {
            let contributions = match mode {
                CompanionContextMode::Compact => sources
                    .compact_summary
                    .map(|(summary, provenance)| {
                        vec![InitialAgentContextContribution::CompactSummary {
                            summary,
                            provenance,
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::WorkflowOnly => sources
                    .workflow
                    .map(|(schema, value, provenance)| {
                        vec![InitialAgentContextContribution::WorkflowContext {
                            schema,
                            value,
                            provenance,
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::ConstraintsOnly => sources
                    .constraints
                    .map(|(schema, value, provenance)| {
                        vec![InitialAgentContextContribution::ConstraintSet {
                            schema,
                            value,
                            provenance,
                        }]
                    })
                    .unwrap_or_default(),
                CompanionContextMode::Full => unreachable!(),
            };
            if contributions.is_empty() {
                return Err(CompanionTargetPlanError::MissingTypedContribution { mode });
            }
            let schema_version = 1;
            let digest = InitialAgentContextPackage::calculate_digest(
                sources.package_id,
                schema_version,
                mode,
                &contributions,
            );
            CompanionRuntimePreparation::FreshCreate {
                initial_context: InitialAgentContextPackage {
                    package_id: sources.package_id,
                    schema_version,
                    mode,
                    contributions,
                    digest,
                },
            }
        }
    };
    Ok(CompanionDispatchTargetPlan {
        preparation,
        adoption_mode,
        first_submit_input: task,
        surface_facts: sources.surface_facts,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionRuntimePreparationEvidence {
    ForkParentHistory {
        child: RuntimeAgentChildIdentity,
        parent_source_coordinate: String,
        through_turn_id: String,
    },
    FreshCreate {
        child: RuntimeAgentChildIdentity,
        context: Option<InitialContextApplicationEvidence>,
    },
}

pub fn verify_companion_activation(
    plan: &CompanionDispatchTargetPlan,
    evidence: &CompanionRuntimePreparationEvidence,
) -> Result<(), CompanionTargetPlanError> {
    match (&plan.preparation, evidence) {
        (
            CompanionRuntimePreparation::ForkParentHistory {
                parent_source_coordinate,
                through_turn_id,
            },
            CompanionRuntimePreparationEvidence::ForkParentHistory {
                parent_source_coordinate: actual_parent,
                through_turn_id: actual_turn,
                ..
            },
        ) if parent_source_coordinate == actual_parent && through_turn_id == actual_turn => Ok(()),
        (
            CompanionRuntimePreparation::FreshCreate { initial_context },
            CompanionRuntimePreparationEvidence::FreshCreate {
                context: Some(actual),
                ..
            },
        ) if actual.package_id == initial_context.package_id
            && actual.package_digest == initial_context.digest =>
        {
            if !initial_context.digest_matches() {
                return Err(CompanionTargetPlanError::InvalidPackageDigest);
            }
            if actual.fidelity == InitialContextDeliveryFidelity::Unsupported {
                return Err(CompanionTargetPlanError::UnsupportedContextFidelity);
            }
            Ok(())
        }
        (
            CompanionRuntimePreparation::FreshCreate { .. },
            CompanionRuntimePreparationEvidence::FreshCreate { context: None, .. },
        ) => Err(CompanionTargetPlanError::MissingContextEvidence),
        (CompanionRuntimePreparation::FreshCreate { .. }, _) => {
            Err(CompanionTargetPlanError::ContextEvidenceMismatch)
        }
        (CompanionRuntimePreparation::ForkParentHistory { .. }, _) => {
            Err(CompanionTargetPlanError::ForkHistoryEvidenceMismatch)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn provenance(authority: &str) -> ContextContributionProvenance {
        ContextContributionProvenance {
            authority: authority.to_owned(),
            source_coordinate: "parent".to_owned(),
            source_revision: "rev-9".to_owned(),
            source_digest: "sha256:source".to_owned(),
        }
    }

    fn sources() -> CompanionContextSources {
        CompanionContextSources {
            parent_source_coordinate: "parent".to_owned(),
            through_turn_id: Some("turn-9".to_owned()),
            package_id: Uuid::new_v4(),
            compact_summary: Some(("summary".to_owned(), provenance("agent_history"))),
            workflow: Some((
                "agentdash.workflow.v1".to_owned(),
                json!({"step": "review"}),
                provenance("workflow"),
            )),
            constraints: Some((
                "agentdash.constraints.v1".to_owned(),
                json!({"deny": ["network"]}),
                provenance("constraint"),
            )),
            surface_facts: json!({"tools": ["read"], "working_directory": "/workspace"}),
        }
    }

    #[test]
    fn full_is_an_exact_parent_history_fork() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "review this".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        assert!(matches!(
            plan.preparation,
            CompanionRuntimePreparation::ForkParentHistory {
                ref parent_source_coordinate,
                ref through_turn_id,
            } if parent_source_coordinate == "parent" && through_turn_id == "turn-9"
        ));
    }

    #[test]
    fn fresh_modes_compile_only_their_typed_package_contribution() {
        let cases = [
            (CompanionContextMode::Compact, "compact_summary"),
            (CompanionContextMode::WorkflowOnly, "workflow_context"),
            (CompanionContextMode::ConstraintsOnly, "constraint_set"),
        ];
        for (mode, expected_kind) in cases {
            let plan = compile_companion_dispatch_target(
                mode,
                CompanionAdoptionMode::BlockingReview,
                SubmitInput {
                    text: "task".to_owned(),
                },
                sources(),
            )
            .expect("plan");
            let CompanionRuntimePreparation::FreshCreate { initial_context } = plan.preparation
            else {
                panic!("fresh create");
            };
            assert!(initial_context.digest_matches());
            assert_eq!(initial_context.contributions.len(), 1);
            let value =
                serde_json::to_value(&initial_context.contributions[0]).expect("contribution json");
            assert_eq!(value["kind"], expected_kind);
            assert!(value.get("surface_facts").is_none());
        }
    }

    #[test]
    fn task_and_surface_facts_are_not_context_package_contributions() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::Compact,
            CompanionAdoptionMode::FollowUpRequired,
            SubmitInput {
                text: "dispatch task".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        assert_eq!(plan.first_submit_input.text, "dispatch task");
        assert_eq!(plan.surface_facts["tools"], json!(["read"]));
        let CompanionRuntimePreparation::FreshCreate { initial_context } = plan.preparation else {
            panic!("fresh");
        };
        let package_json = serde_json::to_value(initial_context).expect("package json");
        assert!(!package_json.to_string().contains("dispatch task"));
        assert!(!package_json.to_string().contains("working_directory"));
    }

    #[test]
    fn fresh_create_activation_requires_exact_package_evidence() {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::WorkflowOnly,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "task".to_owned(),
            },
            sources(),
        )
        .expect("plan");
        let CompanionRuntimePreparation::FreshCreate { initial_context } = &plan.preparation else {
            panic!("fresh");
        };
        let child = RuntimeAgentChildIdentity {
            source_coordinate: "child".to_owned(),
            runtime_agent_id: "runtime-child".to_owned(),
        };
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: child.clone(),
                    context: None,
                }
            ),
            Err(CompanionTargetPlanError::MissingContextEvidence)
        );
        assert_eq!(
            verify_companion_activation(
                &plan,
                &CompanionRuntimePreparationEvidence::FreshCreate {
                    child: child.clone(),
                    context: Some(InitialContextApplicationEvidence {
                        package_id: initial_context.package_id,
                        package_digest: initial_context.digest.clone(),
                        fidelity: InitialContextDeliveryFidelity::Unsupported,
                        materialized_digest: None,
                    }),
                }
            ),
            Err(CompanionTargetPlanError::UnsupportedContextFidelity)
        );
        verify_companion_activation(
            &plan,
            &CompanionRuntimePreparationEvidence::FreshCreate {
                child,
                context: Some(InitialContextApplicationEvidence {
                    package_id: initial_context.package_id,
                    package_digest: initial_context.digest.clone(),
                    fidelity: InitialContextDeliveryFidelity::TypedNative,
                    materialized_digest: Some("sha256:rendered".to_owned()),
                }),
            },
        )
        .expect("matching evidence");
    }

    #[test]
    fn adoption_mode_does_not_change_runtime_preparation() {
        let first = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "task".to_owned(),
            },
            sources(),
        )
        .expect("first");
        let second = compile_companion_dispatch_target(
            CompanionContextMode::Full,
            CompanionAdoptionMode::BlockingReview,
            SubmitInput {
                text: "task".to_owned(),
            },
            sources(),
        )
        .expect("second");
        assert_eq!(first.preparation, second.preparation);
        assert_ne!(first.adoption_mode, second.adoption_mode);
    }
}
