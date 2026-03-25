use serde::{Deserialize, Serialize};

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleStepDefinition, WorkflowAgentRole, WorkflowContract,
    WorkflowDefinition, WorkflowDefinitionSource, WorkflowRecordPolicy, WorkflowTargetKind,
};

pub const TRELLIS_DEV_PROJECT_TEMPLATE_KEY: &str = "trellis_dev_project";
pub const TRELLIS_DEV_STORY_TEMPLATE_KEY: &str = "trellis_dev_story";
pub const TRELLIS_DEV_TASK_TEMPLATE_KEY: &str = "trellis_dev_task";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinWorkflowTemplateBundle {
    pub key: String,
    pub name: String,
    pub description: String,
    pub target_kind: WorkflowTargetKind,
    pub recommended_role: WorkflowAgentRole,
    #[serde(default)]
    pub workflows: Vec<BuiltinWorkflowTemplate>,
    pub lifecycle: BuiltinLifecycleTemplate,
    #[serde(default)]
    pub record_policy: WorkflowRecordPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinWorkflowTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub contract: WorkflowContract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinLifecycleTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub entry_step_key: String,
    #[serde(default)]
    pub steps: Vec<LifecycleStepDefinition>,
}

#[derive(Debug, Clone)]
pub struct BuiltinWorkflowBundle {
    pub workflows: Vec<WorkflowDefinition>,
    pub lifecycle: LifecycleDefinition,
}

impl BuiltinWorkflowTemplateBundle {
    pub fn build_bundle(&self) -> Result<BuiltinWorkflowBundle, String> {
        let workflows = self
            .workflows
            .iter()
            .map(|template| {
                let mut definition = WorkflowDefinition::new(
                    template.key.clone(),
                    template.name.clone(),
                    template.description.clone(),
                    self.target_kind,
                    WorkflowDefinitionSource::BuiltinSeed,
                    template.contract.clone(),
                )?;
                definition.record_policy = self.record_policy.clone();
                definition.recommended_role = Some(self.recommended_role);
                Ok(definition)
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut lifecycle = LifecycleDefinition::new(
            self.lifecycle.key.clone(),
            self.lifecycle.name.clone(),
            self.lifecycle.description.clone(),
            self.target_kind,
            WorkflowDefinitionSource::BuiltinSeed,
            self.lifecycle.entry_step_key.clone(),
            self.lifecycle.steps.clone(),
        )?;
        lifecycle.recommended_role = Some(self.recommended_role);

        Ok(BuiltinWorkflowBundle {
            workflows,
            lifecycle,
        })
    }
}

pub fn list_builtin_workflow_templates() -> Result<Vec<BuiltinWorkflowTemplateBundle>, String> {
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
) -> Result<Option<BuiltinWorkflowTemplateBundle>, String> {
    let template = list_builtin_workflow_templates()?
        .into_iter()
        .find(|item| item.key == builtin_key);
    Ok(template)
}

pub fn build_builtin_workflow_bundle(builtin_key: &str) -> Result<BuiltinWorkflowBundle, String> {
    let template = get_builtin_workflow_template(builtin_key)?
        .ok_or_else(|| format!("workflow template 不存在: {builtin_key}"))?;
    template.build_bundle()
}

fn parse_builtin_workflow_template(raw: &str) -> Result<BuiltinWorkflowTemplateBundle, String> {
    serde_json::from_str::<BuiltinWorkflowTemplateBundle>(raw)
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
    fn builtin_template_can_build_bundle() {
        let bundle =
            build_builtin_workflow_bundle(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("build bundle");

        assert_eq!(bundle.lifecycle.key, TRELLIS_DEV_TASK_TEMPLATE_KEY);
        assert_eq!(bundle.lifecycle.target_kind, WorkflowTargetKind::Task);
        assert_eq!(bundle.workflows.len(), 4);
        assert_eq!(bundle.lifecycle.steps.len(), 4);
    }
}
