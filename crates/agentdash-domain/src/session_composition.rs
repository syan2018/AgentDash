use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionComposition {
    pub persona_label: Option<String>,
    pub persona_prompt: Option<String>,
    #[serde(default)]
    pub workflow_steps: Vec<String>,
    #[serde(default)]
    pub required_context_blocks: Vec<SessionRequiredContextBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRequiredContextBlock {
    pub title: String,
    pub content: String,
}

pub fn validate_session_composition(composition: &SessionComposition) -> Result<(), String> {
    if composition
        .persona_label
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err("session_composition.persona_label 不能为空字符串".to_string());
    }
    if composition
        .persona_prompt
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err("session_composition.persona_prompt 不能为空字符串".to_string());
    }

    for (index, step) in composition.workflow_steps.iter().enumerate() {
        if step.trim().is_empty() {
            return Err(format!(
                "session_composition.workflow_steps[{index}] 不能为空字符串"
            ));
        }
    }

    for (index, block) in composition.required_context_blocks.iter().enumerate() {
        if block.title.trim().is_empty() {
            return Err(format!(
                "session_composition.required_context_blocks[{index}].title 不能为空"
            ));
        }
        if block.content.trim().is_empty() {
            return Err(format!(
                "session_composition.required_context_blocks[{index}].content 不能为空"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_session_composition_rejects_blank_workflow_step() {
        let composition = SessionComposition {
            workflow_steps: vec!["  ".to_string()],
            ..Default::default()
        };
        let error = validate_session_composition(&composition).expect_err("should fail");
        assert!(error.contains("workflow_steps"));
    }
}
