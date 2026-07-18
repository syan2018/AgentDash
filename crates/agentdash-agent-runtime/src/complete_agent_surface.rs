use std::collections::BTreeSet;

use agentdash_agent_service_api::{
    AgentRuntimeOffer, AgentSurfaceRoute, AgentSurfaceSnapshot, BoundAgentSurface,
    BoundAgentSurfaceContribution,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentSurfaceBindingError {
    #[error("Agent surface contribution key is duplicated: {key}")]
    DuplicateContributionKey { key: String },
    #[error("Agent surface contribution {key} has invalid typed semantics or routes")]
    InvalidRequirement { key: String },
    #[error("required Agent surface contribution {key} has no compatible Runtime offer")]
    RequiredContributionUnsupported { key: String },
}

/// Intersects a desired Agent surface with a Runtime offer before any Agent-side materialization.
///
/// The result fixes exactly one causal route for every admitted contribution. Optional
/// contributions that cannot be satisfied are omitted; a missing required contribution rejects
/// the complete binding before `CompleteAgentService::apply_surface` is called.
pub fn bind_complete_agent_surface(
    desired: &AgentSurfaceSnapshot,
    offer: &AgentRuntimeOffer,
) -> Result<BoundAgentSurface, AgentSurfaceBindingError> {
    let mut contributions = Vec::with_capacity(desired.requirements.len());
    let mut keys = BTreeSet::new();

    for requirement in &desired.requirements {
        if !keys.insert(requirement.key.clone()) {
            return Err(AgentSurfaceBindingError::DuplicateContributionKey {
                key: requirement.key.clone(),
            });
        }
        if requirement.allowed_routes.is_empty()
            || !requirement.semantics.matches_payload(&requirement.payload)
        {
            return Err(AgentSurfaceBindingError::InvalidRequirement {
                key: requirement.key.clone(),
            });
        }
        let selected = offer
            .contributions
            .iter()
            .filter(|candidate| {
                candidate.semantics.kind() == requirement.payload.kind()
                    && candidate.semantics.satisfies(&requirement.semantics)
                    && candidate.fidelity.satisfies(requirement.minimum_fidelity)
            })
            .filter_map(|candidate| {
                select_route(&requirement.allowed_routes, &candidate.routes)
                    .filter(|route| {
                        requirement
                            .semantics
                            .required_causal_route()
                            .is_none_or(|required| required == *route)
                            && candidate
                                .semantics
                                .required_causal_route()
                                .is_none_or(|required| required == *route)
                    })
                    .map(|route| (candidate, route))
            })
            .max_by_key(|(candidate, route)| (candidate.fidelity, route_preference(*route)));

        let Some((candidate, route)) = selected else {
            if requirement.required {
                return Err(AgentSurfaceBindingError::RequiredContributionUnsupported {
                    key: requirement.key.clone(),
                });
            }
            continue;
        };

        contributions.push(BoundAgentSurfaceContribution {
            key: requirement.key.clone(),
            required: requirement.required,
            route,
            fidelity: candidate.fidelity,
            semantics: candidate.semantics.clone(),
            payload: requirement.payload.clone(),
            payload_digest: requirement.payload_digest.clone(),
        });
    }

    Ok(BoundAgentSurface {
        revision: desired.revision,
        digest: desired.digest.clone(),
        offer_profile_digest: offer.profile_digest.clone(),
        contributions,
    })
}

fn select_route(
    desired: &BTreeSet<AgentSurfaceRoute>,
    offered: &BTreeSet<AgentSurfaceRoute>,
) -> Option<AgentSurfaceRoute> {
    desired
        .intersection(offered)
        .copied()
        .max_by_key(|route| route_preference(*route))
}

fn route_preference(route: AgentSurfaceRoute) -> u8 {
    match route {
        AgentSurfaceRoute::AgentNativeCallback => 6,
        AgentSurfaceRoute::AgentNativeRegistry => 5,
        AgentSurfaceRoute::RuntimeToolBroker => 4,
        AgentSurfaceRoute::HostLifecycle => 3,
        AgentSurfaceRoute::ImmutableDelivery => 2,
        AgentSurfaceRoute::ObservationOnly => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use agentdash_agent_service_api::{
        AgentConfigurationBoundary, AgentHookAction, AgentHookBlockingSemantics,
        AgentHookDefinitionId, AgentHookEffectKind, AgentHookMutationKind, AgentHookPoint,
        AgentHookSemanticFacet, AgentHookTiming, AgentPayloadDigest, AgentProfileDigest,
        AgentSurfaceCapabilityFacet, AgentSurfaceContributionPayload, AgentSurfaceDigest,
        AgentSurfaceRequirement, AgentSurfaceRevision, AgentSurfaceSemanticFacet,
        AgentToolDelivery, AgentToolName, AgentToolSemanticFacet, AgentToolUpdateSemantics,
        SemanticFidelity,
    };
    use serde_json::json;

    use super::*;

    fn tool_requirement(required: bool) -> AgentSurfaceRequirement {
        AgentSurfaceRequirement {
            key: "tool:search".to_owned(),
            required,
            minimum_fidelity: SemanticFidelity::Exact,
            allowed_routes: BTreeSet::from([
                AgentSurfaceRoute::RuntimeToolBroker,
                AgentSurfaceRoute::AgentNativeCallback,
            ]),
            semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                delivery: AgentToolDelivery::AgentNativeCallback,
                invocation: SemanticFidelity::Exact,
                update: AgentToolUpdateSemantics::BindingOnly,
            }),
            payload: AgentSurfaceContributionPayload::Tool {
                name: AgentToolName::new("search").expect("tool"),
                description: "Search".to_owned(),
                input_schema: json!({"type": "object"}),
                output_schema: None,
            },
            payload_digest: AgentPayloadDigest::new("sha256:tool").expect("payload digest"),
        }
    }

    fn desired(required: bool) -> AgentSurfaceSnapshot {
        AgentSurfaceSnapshot {
            revision: AgentSurfaceRevision(3),
            digest: AgentSurfaceDigest::new("sha256:surface").expect("surface digest"),
            requirements: vec![tool_requirement(required)],
        }
    }

    fn offer(fidelity: SemanticFidelity, routes: BTreeSet<AgentSurfaceRoute>) -> AgentRuntimeOffer {
        AgentRuntimeOffer {
            profile_digest: AgentProfileDigest::new("sha256:profile").expect("profile digest"),
            contributions: vec![AgentSurfaceCapabilityFacet {
                semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                    delivery: AgentToolDelivery::AgentNativeCallback,
                    invocation: fidelity,
                    update: AgentToolUpdateSemantics::BindingOnly,
                }),
                routes,
                fidelity,
                configuration_boundary: AgentConfigurationBoundary::Binding,
            }],
        }
    }

    fn requirement(
        key: &str,
        semantics: AgentSurfaceSemanticFacet,
        payload: AgentSurfaceContributionPayload,
        route: AgentSurfaceRoute,
    ) -> AgentSurfaceRequirement {
        AgentSurfaceRequirement {
            key: key.to_owned(),
            required: true,
            minimum_fidelity: SemanticFidelity::Exact,
            allowed_routes: BTreeSet::from([route]),
            semantics,
            payload,
            payload_digest: AgentPayloadDigest::new(format!("sha256:{key}"))
                .expect("payload digest"),
        }
    }

    fn facet(
        semantics: AgentSurfaceSemanticFacet,
        route: AgentSurfaceRoute,
    ) -> AgentSurfaceCapabilityFacet {
        AgentSurfaceCapabilityFacet {
            semantics,
            routes: BTreeSet::from([route]),
            fidelity: SemanticFidelity::Exact,
            configuration_boundary: AgentConfigurationBoundary::Binding,
        }
    }

    #[test]
    fn required_contribution_is_rejected_before_materialization() {
        let error = bind_complete_agent_surface(
            &desired(true),
            &offer(
                SemanticFidelity::Approximation,
                BTreeSet::from([AgentSurfaceRoute::RuntimeToolBroker]),
            ),
        )
        .expect_err("exact contribution must reject approximation");

        assert_eq!(
            error,
            AgentSurfaceBindingError::RequiredContributionUnsupported {
                key: "tool:search".to_owned()
            }
        );
    }

    #[test]
    fn bound_contribution_has_one_deterministic_causal_route() {
        let bound = bind_complete_agent_surface(
            &desired(true),
            &offer(
                SemanticFidelity::Exact,
                BTreeSet::from([
                    AgentSurfaceRoute::RuntimeToolBroker,
                    AgentSurfaceRoute::AgentNativeCallback,
                ]),
            ),
        )
        .expect("bind surface");

        assert_eq!(bound.contributions.len(), 1);
        assert_eq!(
            bound.contributions[0].route,
            AgentSurfaceRoute::AgentNativeCallback
        );
    }

    #[test]
    fn generic_exact_offer_cannot_substitute_another_tool_delivery_semantic() {
        let mut incompatible = offer(
            SemanticFidelity::Exact,
            BTreeSet::from([
                AgentSurfaceRoute::RuntimeToolBroker,
                AgentSurfaceRoute::AgentNativeCallback,
            ]),
        );
        incompatible.contributions[0].semantics =
            AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                delivery: AgentToolDelivery::RuntimeBrokerCallback,
                invocation: SemanticFidelity::Exact,
                update: AgentToolUpdateSemantics::HotUpdate,
            });

        assert!(matches!(
            bind_complete_agent_surface(&desired(true), &incompatible),
            Err(AgentSurfaceBindingError::RequiredContributionUnsupported { .. })
        ));
    }

    #[test]
    fn hook_effect_semantic_is_compared_before_side_effect() {
        let required_hook = AgentHookSemanticFacet {
            point: AgentHookPoint::BeforeTool,
            timing: AgentHookTiming::Before,
            blocking: AgentHookBlockingSemantics::Blocking {
                fidelity: SemanticFidelity::Exact,
            },
            mutations: BTreeMap::<AgentHookMutationKind, SemanticFidelity>::new(),
            effects: BTreeMap::from([(AgentHookEffectKind::EmitEffect, SemanticFidelity::Exact)]),
        };
        let desired = AgentSurfaceSnapshot {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("hook-surface").expect("surface"),
            requirements: vec![AgentSurfaceRequirement {
                key: "hook:before-tool".to_owned(),
                required: true,
                minimum_fidelity: SemanticFidelity::Exact,
                allowed_routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                semantics: AgentSurfaceSemanticFacet::Hook(required_hook.clone()),
                payload: AgentSurfaceContributionPayload::Hook {
                    definition_id: AgentHookDefinitionId::new("before-tool").expect("hook"),
                    point: AgentHookPoint::BeforeTool,
                    timing: AgentHookTiming::Before,
                    actions: BTreeSet::from([
                        AgentHookAction::AllowOrDeny,
                        AgentHookAction::EmitEffect,
                    ]),
                    deadline_ms: 100,
                },
                payload_digest: AgentPayloadDigest::new("hook-payload").expect("payload"),
            }],
        };
        let mut weaker_hook = required_hook;
        weaker_hook
            .effects
            .insert(AgentHookEffectKind::EmitEffect, SemanticFidelity::Observed);
        let offer = AgentRuntimeOffer {
            profile_digest: AgentProfileDigest::new("profile").expect("profile"),
            contributions: vec![AgentSurfaceCapabilityFacet {
                semantics: AgentSurfaceSemanticFacet::Hook(weaker_hook),
                routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                fidelity: SemanticFidelity::Exact,
                configuration_boundary: AgentConfigurationBoundary::Binding,
            }],
        };

        assert!(matches!(
            bind_complete_agent_surface(&desired, &offer),
            Err(AgentSurfaceBindingError::RequiredContributionUnsupported { .. })
        ));
    }

    #[test]
    fn all_five_surface_kinds_bind_only_from_matching_typed_facets() {
        let tool_semantics = AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
            delivery: AgentToolDelivery::AgentNativeCallback,
            invocation: SemanticFidelity::Exact,
            update: AgentToolUpdateSemantics::BindingOnly,
        });
        let hook_semantics = AgentSurfaceSemanticFacet::Hook(AgentHookSemanticFacet {
            point: AgentHookPoint::BeforeTool,
            timing: AgentHookTiming::Before,
            blocking: AgentHookBlockingSemantics::Blocking {
                fidelity: SemanticFidelity::Exact,
            },
            mutations: BTreeMap::new(),
            effects: BTreeMap::new(),
        });
        let requirements = vec![
            requirement(
                "instruction:system",
                AgentSurfaceSemanticFacet::Instruction,
                AgentSurfaceContributionPayload::Instruction {
                    channel: "system".to_owned(),
                    text: "instruction".to_owned(),
                },
                AgentSurfaceRoute::ImmutableDelivery,
            ),
            requirement(
                "tool:search",
                tool_semantics.clone(),
                AgentSurfaceContributionPayload::Tool {
                    name: AgentToolName::new("search").expect("tool"),
                    description: "Search".to_owned(),
                    input_schema: json!({"type": "object"}),
                    output_schema: None,
                },
                AgentSurfaceRoute::AgentNativeCallback,
            ),
            requirement(
                "hook:before-tool",
                hook_semantics.clone(),
                AgentSurfaceContributionPayload::Hook {
                    definition_id: AgentHookDefinitionId::new("before-tool").expect("hook"),
                    point: AgentHookPoint::BeforeTool,
                    timing: AgentHookTiming::Before,
                    actions: BTreeSet::from([AgentHookAction::AllowOrDeny]),
                    deadline_ms: 100,
                },
                AgentSurfaceRoute::AgentNativeCallback,
            ),
            requirement(
                "workspace:repo",
                AgentSurfaceSemanticFacet::Workspace,
                AgentSurfaceContributionPayload::Workspace {
                    requirement: "repository".to_owned(),
                },
                AgentSurfaceRoute::HostLifecycle,
            ),
            requirement(
                "context:identity",
                AgentSurfaceSemanticFacet::ContextRequirement,
                AgentSurfaceContributionPayload::ContextRequirement {
                    requirement: "identity".to_owned(),
                },
                AgentSurfaceRoute::ImmutableDelivery,
            ),
        ];
        let desired = AgentSurfaceSnapshot {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("five-kinds").expect("surface"),
            requirements,
        };
        let facets = vec![
            facet(
                AgentSurfaceSemanticFacet::Instruction,
                AgentSurfaceRoute::ImmutableDelivery,
            ),
            facet(tool_semantics, AgentSurfaceRoute::AgentNativeCallback),
            facet(hook_semantics, AgentSurfaceRoute::AgentNativeCallback),
            facet(
                AgentSurfaceSemanticFacet::Workspace,
                AgentSurfaceRoute::HostLifecycle,
            ),
            facet(
                AgentSurfaceSemanticFacet::ContextRequirement,
                AgentSurfaceRoute::ImmutableDelivery,
            ),
        ];
        let offer = AgentRuntimeOffer {
            profile_digest: AgentProfileDigest::new("five-kind-profile").expect("profile"),
            contributions: facets,
        };

        let bound = bind_complete_agent_surface(&desired, &offer).expect("bind five kinds");

        assert_eq!(bound.contributions.len(), 5);
        assert_eq!(
            bound
                .contributions
                .iter()
                .map(|contribution| contribution.semantics.kind())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                agentdash_agent_service_api::AgentSurfaceContributionKind::Instruction,
                agentdash_agent_service_api::AgentSurfaceContributionKind::Tool,
                agentdash_agent_service_api::AgentSurfaceContributionKind::Hook,
                agentdash_agent_service_api::AgentSurfaceContributionKind::Workspace,
                agentdash_agent_service_api::AgentSurfaceContributionKind::ContextRequirement,
            ])
        );
    }

    #[test]
    fn required_static_surface_kinds_reject_absent_or_wrong_typed_facets() {
        for (key, semantics, payload, route) in [
            (
                "instruction",
                AgentSurfaceSemanticFacet::Instruction,
                AgentSurfaceContributionPayload::Instruction {
                    channel: "system".to_owned(),
                    text: "instruction".to_owned(),
                },
                AgentSurfaceRoute::ImmutableDelivery,
            ),
            (
                "workspace",
                AgentSurfaceSemanticFacet::Workspace,
                AgentSurfaceContributionPayload::Workspace {
                    requirement: "repository".to_owned(),
                },
                AgentSurfaceRoute::HostLifecycle,
            ),
            (
                "context",
                AgentSurfaceSemanticFacet::ContextRequirement,
                AgentSurfaceContributionPayload::ContextRequirement {
                    requirement: "identity".to_owned(),
                },
                AgentSurfaceRoute::ImmutableDelivery,
            ),
        ] {
            let desired = AgentSurfaceSnapshot {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new(format!("{key}-surface")).expect("surface"),
                requirements: vec![requirement(key, semantics, payload, route)],
            };
            let unrelated = AgentRuntimeOffer {
                profile_digest: AgentProfileDigest::new("unrelated-profile").expect("profile"),
                contributions: vec![facet(
                    AgentSurfaceSemanticFacet::Instruction,
                    AgentSurfaceRoute::ImmutableDelivery,
                )],
            };
            let offer = if key == "instruction" {
                AgentRuntimeOffer {
                    profile_digest: unrelated.profile_digest,
                    contributions: Vec::new(),
                }
            } else {
                unrelated
            };

            assert!(matches!(
                bind_complete_agent_surface(&desired, &offer),
                Err(AgentSurfaceBindingError::RequiredContributionUnsupported { .. })
            ));
        }
    }

    #[test]
    fn duplicate_desired_key_is_rejected() {
        let requirement = tool_requirement(true);
        let desired = AgentSurfaceSnapshot {
            revision: AgentSurfaceRevision(1),
            digest: AgentSurfaceDigest::new("duplicate").expect("surface"),
            requirements: vec![requirement.clone(), requirement],
        };

        assert!(matches!(
            bind_complete_agent_surface(
                &desired,
                &offer(
                    SemanticFidelity::Exact,
                    BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                ),
            ),
            Err(AgentSurfaceBindingError::DuplicateContributionKey { .. })
        ));
    }
}
