use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleEdge, LifecycleStepDefinition, WorkflowBindingKind,
    WorkflowBindingRole, WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource,
};

pub const TRELLIS_DAG_TASK_TEMPLATE_KEY: &str = "trellis_dag_task";
#[allow(dead_code)] // runtime 只用字符串比较；常量用于测试和未来排错引用
pub const BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY: &str = "builtin_workflow_admin";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinWorkflowTemplateBundle {
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    #[serde(default)]
    pub workflows: Vec<BuiltinWorkflowTemplate>,
    pub lifecycle: BuiltinLifecycleTemplate,
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
    #[serde(default)]
    pub edges: Vec<LifecycleEdge>,
}

#[derive(Debug, Clone)]
pub struct BuiltinWorkflowBundle {
    pub workflows: Vec<WorkflowDefinition>,
    pub lifecycle: LifecycleDefinition,
}

impl BuiltinWorkflowTemplateBundle {
    pub fn build_bundle(&self, project_id: Uuid) -> Result<BuiltinWorkflowBundle, String> {
        let workflows = self
            .workflows
            .iter()
            .map(|template| {
                let mut definition = WorkflowDefinition::new(
                    project_id,
                    template.key.clone(),
                    template.name.clone(),
                    template.description.clone(),
                    self.binding_kind,
                    WorkflowDefinitionSource::BuiltinSeed,
                    template.contract.clone(),
                )?;
                definition.recommended_binding_roles = self.recommended_binding_roles.clone();
                Ok(definition)
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut lifecycle = LifecycleDefinition::new(
            project_id,
            self.lifecycle.key.clone(),
            self.lifecycle.name.clone(),
            self.lifecycle.description.clone(),
            self.binding_kind,
            WorkflowDefinitionSource::BuiltinSeed,
            self.lifecycle.entry_step_key.clone(),
            self.lifecycle.steps.clone(),
            self.lifecycle.edges.clone(),
        )?;
        lifecycle.recommended_binding_roles = self.recommended_binding_roles.clone();

        Ok(BuiltinWorkflowBundle {
            workflows,
            lifecycle,
        })
    }
}

pub fn list_builtin_workflow_templates() -> Result<Vec<BuiltinWorkflowTemplateBundle>, String> {
    [
        include_str!("builtins/trellis_dag_task.json"),
        include_str!("builtins/builtin_workflow_admin.json"),
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

pub fn build_builtin_workflow_bundle(
    project_id: Uuid,
    builtin_key: &str,
) -> Result<BuiltinWorkflowBundle, String> {
    let template = get_builtin_workflow_template(builtin_key)?
        .ok_or_else(|| format!("workflow template 不存在: {builtin_key}"))?;
    template.build_bundle(project_id)
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

        assert_eq!(templates.len(), 2);
        let keys = templates
            .iter()
            .map(|item| item.key.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(keys.len(), templates.len());
        assert!(keys.contains(TRELLIS_DAG_TASK_TEMPLATE_KEY));
        assert!(keys.contains(BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY));
    }

    #[test]
    fn builtin_template_can_build_bundle() {
        let bundle = build_builtin_workflow_bundle(Uuid::new_v4(), TRELLIS_DAG_TASK_TEMPLATE_KEY)
            .expect("build bundle");

        assert_eq!(bundle.lifecycle.key, TRELLIS_DAG_TASK_TEMPLATE_KEY);
        assert_eq!(bundle.lifecycle.binding_kind, WorkflowBindingKind::Task);
    }

    #[test]
    fn builtin_workflow_admin_has_expected_shape() {
        use agentdash_domain::workflow::CapabilityDirective;

        let bundle = build_builtin_workflow_bundle(
            Uuid::new_v4(),
            BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY,
        )
        .expect("build builtin_workflow_admin bundle");

        assert_eq!(bundle.lifecycle.key, BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY);
        assert_eq!(
            bundle.lifecycle.binding_kind,
            WorkflowBindingKind::Project,
            "workflow_management 仅在 Project 级 session 可见，lifecycle 必须绑定到 Project"
        );
        assert_eq!(bundle.workflows.len(), 2);
        assert_eq!(bundle.lifecycle.steps.len(), 2);
        assert_eq!(bundle.lifecycle.entry_step_key, "plan");

        let step_keys = bundle
            .lifecycle
            .steps
            .iter()
            .map(|step| step.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(step_keys, vec!["plan", "apply"]);

        // 两步都显式 Add workflow_management 能力，让绑定此 lifecycle 的 Project
        // session 通过新的 workflow_can_grant 授予路径获得 workflow 管理工具集。
        for step in &bundle.lifecycle.steps {
            let adds_workflow_mgmt = step
                .capabilities
                .iter()
                .any(|directive| {
                    matches!(
                        directive,
                        CapabilityDirective::Add(key) if key == "workflow_management"
                    )
                });
            assert!(
                adds_workflow_mgmt,
                "step `{}` 必须 Add workflow_management 能力",
                step.key
            );
        }
    }
}
