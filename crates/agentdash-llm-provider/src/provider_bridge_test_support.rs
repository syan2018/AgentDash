use agentdash_agent::bridge::BridgeRequest;
use agentdash_agent::types::{AgentMessage, ToolDefinition};

pub(crate) const SYSTEM_PROMPT: &str = "  SYSTEM-PROMPT-MUST-STAY-BYTE-EXACT\nsecond system line  ";
pub(crate) const USER_PROMPT: &str = "USER-PROMPT-MUST-STAY-USER-OWNED";
pub(crate) const TOOL_NAME: &str = "structured_contract_guard";
pub(crate) const TOOL_DESCRIPTION: &str = "TOOL-DESCRIPTION-MUST-STAY-IN-VENDOR-STRUCTURED-FIELD";
pub(crate) const SCHEMA_PROPERTY: &str = "schema_only_nested_value";
pub(crate) const SCHEMA_ENUM_VALUE: &str = "SCHEMA-ENUM-MUST-STAY-STRUCTURED";

pub(crate) fn bridge_request() -> BridgeRequest {
    BridgeRequest {
        system_prompt: Some(SYSTEM_PROMPT.to_string()),
        messages: vec![AgentMessage::user(USER_PROMPT)],
        tools: vec![ToolDefinition {
            name: TOOL_NAME.to_string(),
            description: TOOL_DESCRIPTION.to_string(),
            parameters: tool_parameters(),
        }],
        thinking_level: None,
    }
}

pub(crate) fn tool_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "schema_only_nested_value": {
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": [SCHEMA_ENUM_VALUE]
                    }
                },
                "required": ["mode"],
                "additionalProperties": false
            }
        },
        "required": [SCHEMA_PROPERTY],
        "additionalProperties": false
    })
}

pub(crate) fn serialized_body(body: serde_json::Value) -> serde_json::Value {
    let wire = serde_json::to_vec(&body).expect("provider request body should serialize");
    serde_json::from_slice(&wire)
        .expect("serialized provider request body should remain valid JSON")
}

pub(crate) fn assert_prompt_lanes_exclude_tool_metadata(prompt_lanes: &[&serde_json::Value]) {
    for lane in prompt_lanes {
        let serialized =
            serde_json::to_string(lane).expect("provider prompt lane should serialize");
        for structured_only_marker in [
            TOOL_NAME,
            TOOL_DESCRIPTION,
            SCHEMA_PROPERTY,
            SCHEMA_ENUM_VALUE,
        ] {
            assert!(
                !serialized.contains(structured_only_marker),
                "structured tool metadata leaked into a provider prompt lane: {serialized}"
            );
        }
    }
}
