use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityDefinition, ActivityTransition, AgentProcedure, DefinitionSource, AgentProcedureContract,
    WorkflowGraph,
};

pub const TRELLIS_DAG_TASK_TEMPLATE_KEY: &str = "trellis_dag_task";
#[cfg(test)]
pub const BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY: &str = "builtin_workflow_admin";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinWorkflowTemplateBundle {
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub workflows: Vec<BuiltinWorkflowTemplate>,
    pub graph: BuiltinLifecycleTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinWorkflowTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub contract: AgentProcedureContract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinLifecycleTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub entry_activity_key: String,
    #[serde(default)]
    pub activities: Vec<ActivityDefinition>,
    #[serde(default)]
    pub transitions: Vec<ActivityTransition>,
}

#[derive(Debug, Clone)]
pub struct BuiltinWorkflowBundle {
    pub procedures: Vec<AgentProcedure>,
    pub graph: WorkflowGraph,
}

impl BuiltinWorkflowTemplateBundle {
    pub fn build_bundle(&self, project_id: Uuid) -> Result<BuiltinWorkflowBundle, String> {
        let procedures = self
            .workflows
            .iter()
            .map(|template| {
                AgentProcedure::new(
                    project_id,
                    template.key.clone(),
                    template.name.clone(),
                    template.description.clone(),
                    DefinitionSource::BuiltinSeed,
                    template.contract.clone(),
                )
            })
            .collect::<Result<Vec<_>, String>>()?;

        let graph = WorkflowGraph::new(
            project_id,
            self.graph.key.clone(),
            self.graph.name.clone(),
            self.graph.description.clone(),
            DefinitionSource::BuiltinSeed,
            self.graph.entry_activity_key.clone(),
            self.graph.activities.clone(),
            self.graph.transitions.clone(),
        )?;

        Ok(BuiltinWorkflowBundle { procedures, graph })
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

        assert_eq!(bundle.graph.key, TRELLIS_DAG_TASK_TEMPLATE_KEY);
    }

    #[test]
    fn builtin_workflow_admin_has_expected_shape() {
        let bundle =
            build_builtin_workflow_bundle(Uuid::new_v4(), BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY)
                .expect("build builtin_workflow_admin bundle");

        assert_eq!(bundle.graph.key, BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY);
        assert_eq!(bundle.procedures.len(), 2);
        assert_eq!(bundle.graph.activities.len(), 2);
        assert_eq!(bundle.graph.entry_activity_key, "plan");

        let activity_keys = bundle
            .graph
            .activities
            .iter()
            .map(|activity| activity.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(activity_keys, vec!["plan", "apply"]);

        // 必须显式声明 plan → apply 的 flow transition，确保调度器可确定下一 Activity。
        assert_eq!(bundle.graph.transitions.len(), 1);
        let transition = &bundle.graph.transitions[0];
        assert_eq!(transition.from, "plan");
        assert_eq!(transition.to, "apply");
        assert!(transition.artifact_bindings.is_empty());

        // 工具能力声明统一进入 workflow.contract.capability_config.tool_directives。
        // 每个 workflow 都必须显式声明 workflow_management，让绑定此 lifecycle 的 Project
        // session 在启动时拿到 workflow 管理工具集。
        for workflow in &bundle.procedures {
            assert!(
                workflow
                    .contract
                    .capability_config
                    .tool_directives
                    .iter()
                    .any(|d| d.is_add() && d.key() == "workflow_management"),
                "workflow `{}` 必须声明 workflow_management 能力",
                workflow.key
            );
        }

        let plan = bundle
            .procedures
            .iter()
            .find(|p| p.key == "builtin_workflow_admin_plan")
            .expect("plan procedure exists");
        let apply = bundle
            .procedures
            .iter()
            .find(|p| p.key == "builtin_workflow_admin_apply")
            .expect("apply procedure exists");
        for tool in ["upsert_workflow_tool", "upsert_lifecycle_tool"] {
            assert!(
                plan.contract
                    .capability_config
                    .tool_directives
                    .iter()
                    .any(|directive| directive.is_remove()
                        && directive.key() == "workflow_management"
                        && directive.path().tool.as_deref() == Some(tool)),
                "Plan 阶段必须屏蔽 workflow_management::{tool}"
            );
            assert!(
                !apply
                    .contract
                    .capability_config
                    .tool_directives
                    .iter()
                    .any(|directive| directive.is_remove()
                        && directive.key() == "workflow_management"
                        && directive.path().tool.as_deref() == Some(tool)),
                "Apply 阶段不得继续屏蔽 workflow_management::{tool}"
            );
        }
    }
}
