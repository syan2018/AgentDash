use agentdash_agent_runtime_contract::managed_projection::{
    ManagedRuntimeChangePage, ManagedRuntimeSnapshot,
};
use agentdash_agent_runtime_contract::{RuntimeChangeSequence, RuntimeThreadId};
use async_trait::async_trait;

use super::{AgentRunForkRuntimePort, RuntimeAgentChildIdentity, SubmitInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunActivationWorkstream {
    W7ProductCaller,
    W8LegacyBoundary,
    W7ProductCallerAndW8Cleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunActivationOwner {
    RuntimeContract,
    BusinessSurface,
    RuntimeToolBroker,
    NativeAdapter,
    ProductCaller,
    LegacyDeletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunConsumerActivation {
    pub consumer: &'static str,
    pub current_artifact: &'static str,
    pub workstream: AgentRunActivationWorkstream,
    pub final_owner: AgentRunActivationOwner,
    pub action: &'static str,
    pub activation_artifact: &'static str,
    pub prerequisite: &'static str,
}

/// The exact direct-consumer closure recorded by
/// `research/agent-types-hard-cut-consumer-inventory.md`.
pub const AGENT_RUN_TARGET_ACTIVATION_MANIFEST: &[AgentRunConsumerActivation] = &[
    AgentRunConsumerActivation {
        consumer: "agentdash-agent-protocol",
        current_artifact: "Core input/transcript projection",
        workstream: AgentRunActivationWorkstream::W8LegacyBoundary,
        final_owner: AgentRunActivationOwner::NativeAdapter,
        action: "move the anti-corruption projection to Native and delete the legacy protocol crate",
        activation_artifact: "Native-owned input and transcript projector",
        prerequisite: "all W7 callers consume Runtime Contract and Tool Broker boundaries",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-api",
        current_artifact: "Core tool object bootstrap composition",
        workstream: AgentRunActivationWorkstream::W7ProductCaller,
        final_owner: AgentRunActivationOwner::ProductCaller,
        action: "compose Business Surface, Runtime Tool Broker, and AgentHostCallbacks",
        activation_artifact: "product Runtime composition root",
        prerequisite: "managed Runtime snapshot/change contract and Host callback receipts",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-application",
        current_artifact: "AgentTool implementations and RuntimeToolProvider catalog",
        workstream: AgentRunActivationWorkstream::W7ProductCaller,
        final_owner: AgentRunActivationOwner::BusinessSurface,
        action: "publish typed tool contributions and requirements",
        activation_artifact: "Business Surface tool contribution catalog",
        prerequisite: "Runtime Tool Broker contribution materialization",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-application-agentrun",
        current_artifact: "Core tool introspection and journal transcript reconstruction",
        workstream: AgentRunActivationWorkstream::W7ProductCaller,
        final_owner: AgentRunActivationOwner::RuntimeContract,
        action: "read Surface contributions and Runtime snapshot/change/context contracts",
        activation_artifact: "AgentRun Runtime Contract ports and durable orchestration",
        prerequisite: "managed Runtime projection and exact fork/fresh context receipts",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-application-lifecycle",
        current_artifact: "workflow AgentTool and RuntimeToolProvider",
        workstream: AgentRunActivationWorkstream::W7ProductCaller,
        final_owner: AgentRunActivationOwner::BusinessSurface,
        action: "publish workflow tool contributions",
        activation_artifact: "workflow Business Surface contribution",
        prerequisite: "Runtime Tool Broker owns execution and Runtime change owns presentation",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-application-ports",
        current_artifact: "MCP DynAgentTool discovery and RuntimeSession live port",
        workstream: AgentRunActivationWorkstream::W7ProductCallerAndW8Cleanup,
        final_owner: AgentRunActivationOwner::RuntimeToolBroker,
        action: "return platform descriptors and call routes, then delete RuntimeSession live ports",
        activation_artifact: "typed MCP descriptor and Tool Broker call route",
        prerequisite: "all MCP execution callers use Runtime commands",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-application-runtime-gateway",
        current_artifact: "Core tool/session/MCP gateway modules",
        workstream: AgentRunActivationWorkstream::W8LegacyBoundary,
        final_owner: AgentRunActivationOwner::LegacyDeletion,
        action: "retain extension actions under the extension gateway and delete legacy modules",
        activation_artifact: "extension gateway without Agent Runtime responsibilities",
        prerequisite: "Runtime commands and Tool Broker own session/tool/MCP execution",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-application-vfs",
        current_artifact: "filesystem AgentTool and ToolProtocolProjector implementations",
        workstream: AgentRunActivationWorkstream::W7ProductCaller,
        final_owner: AgentRunActivationOwner::BusinessSurface,
        action: "publish VFS tool contributions without Core traits",
        activation_artifact: "VFS Business Surface contribution",
        prerequisite: "Runtime Tool Broker invokes VFS contribution routes",
    },
    AgentRunConsumerActivation {
        consumer: "agentdash-spi",
        current_artifact: "Agent re-export facade and connector tool assembly",
        workstream: AgentRunActivationWorkstream::W8LegacyBoundary,
        final_owner: AgentRunActivationOwner::LegacyDeletion,
        action: "retain non-Agent platform ports and delete the Agent facade and connector assembly",
        activation_artifact: "platform SPI without Agent types",
        prerequisite: "W7 product callers no longer consume assembled Core tools",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunGeneratedArtifactKind {
    RustRuntimeContract,
    TypeScriptApiBindings,
    ContractDiffGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunGeneratedArtifactActivation {
    pub artifact: AgentRunGeneratedArtifactKind,
    pub owner: AgentRunActivationOwner,
    pub activation_action: &'static str,
    pub prerequisite: &'static str,
    pub target_evidence: &'static str,
}

pub const AGENT_RUN_TARGET_GENERATED_ARTIFACTS: &[AgentRunGeneratedArtifactActivation] = &[
    AgentRunGeneratedArtifactActivation {
        artifact: AgentRunGeneratedArtifactKind::RustRuntimeContract,
        owner: AgentRunActivationOwner::RuntimeContract,
        activation_action: "consume managed_projection as the sole snapshot/change vocabulary",
        prerequisite: "W3 Runtime Contract milestone",
        target_evidence: "activation/w7-product-protocol/managed-runtime-projection.schema.json",
    },
    AgentRunGeneratedArtifactActivation {
        artifact: AgentRunGeneratedArtifactKind::TypeScriptApiBindings,
        owner: AgentRunActivationOwner::RuntimeContract,
        activation_action: "regenerate frontend bindings from the canonical Rust schema",
        prerequisite: "production routes expose only managed Runtime projection types",
        target_evidence: "packages/app-web/src/features/session/model/fixtures/managedRuntimeProjection.json",
    },
    AgentRunGeneratedArtifactActivation {
        artifact: AgentRunGeneratedArtifactKind::ContractDiffGate,
        owner: AgentRunActivationOwner::RuntimeContract,
        activation_action: "require a clean generated-contract diff in the atomic cutover",
        prerequisite: "Rust routes and TypeScript consumers are switched together",
        target_evidence: "activation/w7-product-protocol/manifest.json",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunS5Prerequisite {
    ManagedRuntimeProjection,
    CompleteAgentCapabilitySignoff,
    DurableOrchestrationFaultTests,
    ProductCallerCutoverTests,
    GeneratedContractParity,
    AtomicProductionComposition,
    W8LegacyDeletionAndSchema,
}

pub const AGENT_RUN_TARGET_S5_PREREQUISITES: &[AgentRunS5Prerequisite] = &[
    AgentRunS5Prerequisite::ManagedRuntimeProjection,
    AgentRunS5Prerequisite::CompleteAgentCapabilitySignoff,
    AgentRunS5Prerequisite::DurableOrchestrationFaultTests,
    AgentRunS5Prerequisite::ProductCallerCutoverTests,
    AgentRunS5Prerequisite::GeneratedContractParity,
    AgentRunS5Prerequisite::AtomicProductionComposition,
    AgentRunS5Prerequisite::W8LegacyDeletionAndSchema,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunS5ActivationGate {
    pub order: u8,
    pub prerequisite: AgentRunS5Prerequisite,
    pub owner: AgentRunActivationOwner,
    pub evidence: &'static str,
    pub negative_gate: Option<&'static str>,
}

/// Atomic activation order. Every owner supplies its evidence before the next
/// gate advances; S5 therefore cannot expose a partial product/runtime cut.
pub const AGENT_RUN_TARGET_S5_ACTIVATION_GATES: &[AgentRunS5ActivationGate] = &[
    AgentRunS5ActivationGate {
        order: 1,
        prerequisite: AgentRunS5Prerequisite::ManagedRuntimeProjection,
        owner: AgentRunActivationOwner::RuntimeContract,
        evidence: "managed_projection snapshot/change/availability schema and typed gap tests",
        negative_gate: Some(
            "Product protocol contains no concrete Runtime or service-api source identity",
        ),
    },
    AgentRunS5ActivationGate {
        order: 2,
        prerequisite: AgentRunS5Prerequisite::CompleteAgentCapabilitySignoff,
        owner: AgentRunActivationOwner::NativeAdapter,
        evidence: "S3 exact fork, inspect, fresh context, activation, and submission receipts",
        negative_gate: Some("Product request types do not cross the Complete Agent API boundary"),
    },
    AgentRunS5ActivationGate {
        order: 3,
        prerequisite: AgentRunS5Prerequisite::DurableOrchestrationFaultTests,
        owner: AgentRunActivationOwner::ProductCaller,
        evidence: "Fork and Companion crash/CAS/unknown-outcome restart matrices",
        negative_gate: Some("Known child identity cannot dispatch a second create or fork effect"),
    },
    AgentRunS5ActivationGate {
        order: 4,
        prerequisite: AgentRunS5Prerequisite::ProductCallerCutoverTests,
        owner: AgentRunActivationOwner::BusinessSurface,
        evidence: "all six W7 consumer entries pass production caller tests",
        negative_gate: Some("W7 callers contain no Core AgentTool or RuntimeToolProvider assembly"),
    },
    AgentRunS5ActivationGate {
        order: 5,
        prerequisite: AgentRunS5Prerequisite::GeneratedContractParity,
        owner: AgentRunActivationOwner::RuntimeContract,
        evidence: "Rust canonical fixture, TypeScript consumer, and generated-contract diff agree",
        negative_gate: Some("canonical generated artifact diff is empty after regeneration"),
    },
    AgentRunS5ActivationGate {
        order: 6,
        prerequisite: AgentRunS5Prerequisite::AtomicProductionComposition,
        owner: AgentRunActivationOwner::ProductCaller,
        evidence: "Business Surface, Tool Broker, Host callbacks, and Runtime read ports bind together",
        negative_gate: Some("production constructor requires every final port explicitly"),
    },
    AgentRunS5ActivationGate {
        order: 7,
        prerequisite: AgentRunS5Prerequisite::W8LegacyDeletionAndSchema,
        owner: AgentRunActivationOwner::LegacyDeletion,
        evidence: "nine-consumer closure, final migration, crate DAG, and composition gates",
        negative_gate: Some("agentdash-agent-types direct consumer count is zero"),
    },
];

#[async_trait]
pub trait AgentRunBusinessSurfacePort: Send + Sync {
    async fn apply_business_surface(
        &self,
        child: &RuntimeAgentChildIdentity,
        surface_facts: &serde_json::Value,
    ) -> Result<String, String>;
}

#[async_trait]
pub trait AgentRunToolBrokerPort: Send + Sync {
    async fn bind_tool_broker(&self, child: &RuntimeAgentChildIdentity) -> Result<String, String>;
}

#[async_trait]
pub trait AgentRunHostCallbacksPort: Send + Sync {
    async fn submit_input(
        &self,
        child: &RuntimeAgentChildIdentity,
        input: SubmitInput,
    ) -> Result<String, String>;
}

#[async_trait]
pub trait AgentRunRuntimeProjectionPort: Send + Sync {
    async fn load_snapshot(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeSnapshot, String>;

    async fn load_changes(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<ManagedRuntimeChangePage, String>;
}

/// S5 显式 composition boundary。生产 constructor 必须完整注入这些 final ports，
/// 不存在 legacy/default 分支。
pub struct AgentRunProductProtocolPorts<'a> {
    pub runtime: &'a dyn AgentRunForkRuntimePort,
    pub business_surface: &'a dyn AgentRunBusinessSurfacePort,
    pub tool_broker: &'a dyn AgentRunToolBrokerPort,
    pub host_callbacks: &'a dyn AgentRunHostCallbacksPort,
    pub runtime_projection: &'a dyn AgentRunRuntimeProjectionPort,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const EXACT_CONSUMERS: [&str; 9] = [
        "agentdash-agent-protocol",
        "agentdash-api",
        "agentdash-application",
        "agentdash-application-agentrun",
        "agentdash-application-lifecycle",
        "agentdash-application-ports",
        "agentdash-application-runtime-gateway",
        "agentdash-application-vfs",
        "agentdash-spi",
    ];

    #[test]
    fn activation_manifest_matches_the_nine_direct_consumers() {
        assert_eq!(AGENT_RUN_TARGET_ACTIVATION_MANIFEST.len(), 9);
        let actual = AGENT_RUN_TARGET_ACTIVATION_MANIFEST
            .iter()
            .map(|entry| entry.consumer)
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, EXACT_CONSUMERS.into_iter().collect());
    }

    #[test]
    fn activation_manifest_covers_w7_and_w8_closure_without_empty_evidence() {
        let w7 = AGENT_RUN_TARGET_ACTIVATION_MANIFEST
            .iter()
            .filter(|entry| {
                matches!(
                    entry.workstream,
                    AgentRunActivationWorkstream::W7ProductCaller
                        | AgentRunActivationWorkstream::W7ProductCallerAndW8Cleanup
                )
            })
            .map(|entry| entry.consumer)
            .collect::<BTreeSet<_>>();
        let w8 = AGENT_RUN_TARGET_ACTIVATION_MANIFEST
            .iter()
            .filter(|entry| {
                matches!(
                    entry.workstream,
                    AgentRunActivationWorkstream::W8LegacyBoundary
                        | AgentRunActivationWorkstream::W7ProductCallerAndW8Cleanup
                )
            })
            .map(|entry| entry.consumer)
            .collect::<BTreeSet<_>>();

        assert_eq!(
            w7,
            [
                "agentdash-api",
                "agentdash-application",
                "agentdash-application-agentrun",
                "agentdash-application-lifecycle",
                "agentdash-application-ports",
                "agentdash-application-vfs",
            ]
            .into_iter()
            .collect()
        );
        for artifact in AGENT_RUN_TARGET_GENERATED_ARTIFACTS {
            assert!(!artifact.target_evidence.is_empty());
        }
        assert_eq!(
            w8,
            [
                "agentdash-agent-protocol",
                "agentdash-application-ports",
                "agentdash-application-runtime-gateway",
                "agentdash-spi",
            ]
            .into_iter()
            .collect()
        );

        for entry in AGENT_RUN_TARGET_ACTIVATION_MANIFEST {
            assert!(!entry.current_artifact.is_empty());
            assert!(!entry.action.is_empty());
            assert!(!entry.activation_artifact.is_empty());
            assert!(!entry.prerequisite.is_empty());
        }
    }

    #[test]
    fn generated_contract_activation_requires_rust_types_typescript_and_diff_gate() {
        let artifacts = AGENT_RUN_TARGET_GENERATED_ARTIFACTS
            .iter()
            .map(|entry| entry.artifact)
            .collect::<Vec<_>>();
        assert_eq!(
            artifacts,
            vec![
                AgentRunGeneratedArtifactKind::RustRuntimeContract,
                AgentRunGeneratedArtifactKind::TypeScriptApiBindings,
                AgentRunGeneratedArtifactKind::ContractDiffGate,
            ]
        );
        assert!(
            AGENT_RUN_TARGET_S5_PREREQUISITES
                .contains(&AgentRunS5Prerequisite::W8LegacyDeletionAndSchema)
        );
    }

    #[test]
    fn s5_gates_have_explicit_owner_order_and_negative_evidence() {
        assert_eq!(
            AGENT_RUN_TARGET_S5_ACTIVATION_GATES
                .iter()
                .map(|gate| gate.order)
                .collect::<Vec<_>>(),
            (1..=7).collect::<Vec<_>>()
        );
        assert_eq!(
            AGENT_RUN_TARGET_S5_ACTIVATION_GATES
                .iter()
                .map(|gate| gate.prerequisite)
                .collect::<Vec<_>>(),
            AGENT_RUN_TARGET_S5_PREREQUISITES
        );
        for gate in AGENT_RUN_TARGET_S5_ACTIVATION_GATES {
            assert!(!gate.evidence.is_empty());
            assert!(gate.negative_gate.is_some_and(|gate| !gate.is_empty()));
        }
    }
}
