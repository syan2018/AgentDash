use agentdash_agent::dash::{DashSurfaceInstruction, DashToolDefinition};
use agentdash_agent_protocol::AgentSurfaceInstructionPresentation;
use agentdash_agent_service_api::AgentProfileDigest;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(crate) const DASH_INTRINSIC_INSTRUCTION_KEY: &str = "native:dash-agent:base-system-prompt";
pub(crate) const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/default_system_prompt.md");

pub(crate) fn instruction() -> DashSurfaceInstruction {
    DashSurfaceInstruction {
        key: DASH_INTRINSIC_INSTRUCTION_KEY.to_owned(),
        channel: "system".to_owned(),
        text: DEFAULT_SYSTEM_PROMPT.trim().to_owned(),
        presentation: AgentSurfaceInstructionPresentation::Identity,
    }
}

pub(crate) fn profile_digest() -> AgentProfileDigest {
    AgentProfileDigest::new(format!(
        "dash-agent-profile-v2:intrinsic-sha256:{:x}",
        Sha256::digest(DEFAULT_SYSTEM_PROMPT.as_bytes())
    ))
    .expect("the embedded Dash intrinsic prompt produces a profile digest")
}

pub(crate) fn materialization_digest(
    instructions: &[DashSurfaceInstruction],
    tools: &[DashToolDefinition],
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct MaterializedSurface<'a> {
        schema: &'static str,
        instructions: &'a [DashSurfaceInstruction],
        tools: &'a [DashToolDefinition],
    }

    let encoded = serde_json::to_vec(&MaterializedSurface {
        schema: "agentdash.dash-materialized-surface/v1",
        instructions,
        tools,
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intrinsic_instruction_and_profile_share_the_embedded_prompt() {
        let instruction = instruction();

        assert_eq!(instruction.key, DASH_INTRINSIC_INSTRUCTION_KEY);
        assert_eq!(instruction.text, DEFAULT_SYSTEM_PROMPT.trim());
        assert!(
            instruction
                .text
                .contains("share brief one-sentence progress updates")
        );
        assert!(profile_digest().as_str().contains("intrinsic-sha256"));
    }

    #[test]
    fn materialization_digest_covers_intrinsic_content() {
        let original = instruction();
        let mut changed = original.clone();
        changed.text.push_str("\nchanged");

        let original_digest = materialization_digest(&[original], &[]).unwrap();
        let changed_digest = materialization_digest(&[changed], &[]).unwrap();

        assert_ne!(original_digest, changed_digest);
    }
}
