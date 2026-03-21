use serde::{Deserialize, Serialize};

use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowDefinition, WorkflowPhaseDefinition, WorkflowRecordPolicy,
    WorkflowTargetKind,
};

pub const TRELLIS_DEV_PROJECT_TEMPLATE_KEY: &str = "trellis_dev_project";
pub const TRELLIS_DEV_STORY_TEMPLATE_KEY: &str = "trellis_dev_story";
pub const TRELLIS_DEV_TASK_TEMPLATE_KEY: &str = "trellis_dev_task";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinWorkflowTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    pub recommended_role: WorkflowAgentRole,
    #[serde(default)]
    pub phases: Vec<WorkflowPhaseDefinition>,
    #[serde(default)]
    pub record_policy: WorkflowRecordPolicy,
}

impl BuiltinWorkflowTemplate {
    pub fn build_definition(&self) -> Result<WorkflowDefinition, String> {
        let mut definition = WorkflowDefinition::new(
            self.key.clone(),
            self.name.clone(),
            self.description.clone(),
            self.target_kind,
            self.phases.clone(),
        )?;
        definition.record_policy = self.record_policy.clone();
        Ok(definition)
    }
}

pub fn list_builtin_workflow_templates() -> Result<Vec<BuiltinWorkflowTemplate>, String> {
    [
        include_str!("builtins/trellis_dev_project.json"),
        include_str!("builtins/trellis_dev_story.json"),
        include_str!("builtins/trellis_dev_task.json"),
    ]
    .into_iter()
    .map(parse_builtin_workflow_template)
    .collect()
}

pub fn get_builtin_workflow_template(
    builtin_key: &str,
) -> Result<Option<BuiltinWorkflowTemplate>, String> {
    let template = list_builtin_workflow_templates()?
        .into_iter()
        .find(|item| item.key == builtin_key);
    Ok(template)
}

pub fn build_builtin_workflow_definition(builtin_key: &str) -> Result<WorkflowDefinition, String> {
    let template = get_builtin_workflow_template(builtin_key)?
        .ok_or_else(|| format!("workflow template 不存在: {builtin_key}"))?;
    template.build_definition()
}

fn parse_builtin_workflow_template(raw: &str) -> Result<BuiltinWorkflowTemplate, String> {
    serde_json::from_str::<BuiltinWorkflowTemplate>(raw)
        .map_err(|error| format!("解析 builtin workflow template 失败: {error}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn builtin_workflow_templates_are_unique_and_loadable() {
        let templates = list_builtin_workflow_templates().expect("load templates");

        assert_eq!(templates.len(), 3);
        let keys = templates
            .iter()
            .map(|item| item.key.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(keys.len(), templates.len());
        assert!(keys.contains(TRELLIS_DEV_PROJECT_TEMPLATE_KEY));
        assert!(keys.contains(TRELLIS_DEV_STORY_TEMPLATE_KEY));
        assert!(keys.contains(TRELLIS_DEV_TASK_TEMPLATE_KEY));
    }

    #[test]
    fn builtin_template_can_build_definition() {
        let definition = build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY)
            .expect("build definition");

        assert_eq!(definition.key, TRELLIS_DEV_TASK_TEMPLATE_KEY);
        assert_eq!(definition.target_kind, WorkflowTargetKind::Task);
        assert_eq!(definition.phases.len(), 4);
        assert_eq!(
            definition.phases[3].default_artifact_title.as_deref(),
            Some("阶段总结")
        );
    }
}
