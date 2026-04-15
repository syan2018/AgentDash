use agentdash_domain::{project::Project, story::Story, task::Task, workspace::Workspace};
use serde_json::{Map, Value};

use crate::runtime::{AddressSpace, AgentConfig, RuntimeMcpServer};
use crate::session::bootstrap::{
    BootstrapOwnerVariant, BootstrapPlanInput, build_bootstrap_plan,
    derive_session_context_snapshot,
};
use crate::session::context::{SessionContextSnapshot, extract_story_overrides};
use crate::workflow::{ActiveWorkflowProjection, workflow_artifact_type_tag};

pub fn build_task_execution_snapshot(
    task: &Task,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    resolved_config: Option<&AgentConfig>,
    executor_source: &str,
    executor_resolution_error: Option<String>,
    address_space: &AddressSpace,
    mcp_servers: &[RuntimeMcpServer],
    workflow: Option<&ActiveWorkflowProjection>,
) -> SessionContextSnapshot {
    let plan = build_bootstrap_plan(BootstrapPlanInput {
        project: project.clone(),
        story: Some(story.clone()),
        workspace: workspace.cloned(),
        resolved_config: resolved_config.cloned(),
        address_space: Some(address_space.clone()),
        mcp_servers: mcp_servers.to_vec(),
        working_dir: None,
        executor_preset_name: task.agent_binding.preset_name.clone(),
        executor_source: executor_source.to_string(),
        executor_resolution_error,
        owner_variant: BootstrapOwnerVariant::Task {
            story_overrides: extract_story_overrides(story),
        },
        workflow: workflow.cloned(),
    });
    derive_session_context_snapshot(&plan)
}

pub fn build_task_runtime_context_entries(
    task: &Task,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    execution_snapshot: &SessionContextSnapshot,
    workflow: Option<&ActiveWorkflowProjection>,
) -> Map<String, Value> {
    let mut entries = Map::new();

    insert_text_entry(
        &mut entries,
        "execution_context",
        build_task_execution_context_body(
            task,
            story,
            project,
            workspace,
            execution_snapshot,
            workflow,
        ),
    );
    insert_text_entry(
        &mut entries,
        "story_context_snapshot",
        build_story_context_snapshot_body(story),
    );

    if let Some(prd) = story
        .context
        .prd_doc
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        insert_text_entry(&mut entries, "story_prd", prd.to_string());
    }

    if let Some(active_workflow) = workflow {
        insert_text_entry(
            &mut entries,
            "review_checklist",
            build_review_checklist_body(active_workflow),
        );
        insert_text_entry(
            &mut entries,
            "workspace_journal",
            build_workspace_journal_body(),
        );
        insert_text_entry(
            &mut entries,
            "workflow_archive_action",
            build_workflow_archive_action_body(task, active_workflow),
        );
    }

    entries
}

fn insert_text_entry(entries: &mut Map<String, Value>, key: &str, content: String) {
    if !content.trim().is_empty() {
        entries.insert(key.to_string(), Value::String(content));
    }
}

fn build_task_execution_context_body(
    task: &Task,
    story: &Story,
    project: &Project,
    workspace: Option<&Workspace>,
    execution_snapshot: &SessionContextSnapshot,
    workflow: Option<&ActiveWorkflowProjection>,
) -> String {
    let workspace_id = workspace
        .map(|item| item.id.to_string())
        .unwrap_or_else(|| "-".to_string());
    let active_workflow = workflow
        .map(|item| format!("{} / {}", item.lifecycle.key, item.active_step.key))
        .unwrap_or_else(|| "-".to_string());
    let snapshot_json =
        serde_json::to_string_pretty(execution_snapshot).unwrap_or_else(|_| "{}".to_string());

    format!(
        "- owner: `task`\n- project_id: `{}`\n- project_name: {}\n- story_id: `{}`\n- story_title: {}\n- task_id: `{}`\n- task_title: {}\n- workspace_id: `{}`\n- active_workflow: {}\n\n### Session Context Snapshot\n```json\n{}\n```",
        project.id,
        project.name.trim(),
        story.id,
        story.title.trim(),
        task.id,
        task.title.trim(),
        workspace_id,
        active_workflow,
        snapshot_json
    )
}

fn build_story_context_snapshot_body(story: &Story) -> String {
    let snapshot_json =
        serde_json::to_string_pretty(&story.context).unwrap_or_else(|_| "{}".to_string());
    format!(
        "- story_id: `{}`\n- story_title: {}\n\n### Story Context\n```json\n{}\n```",
        story.id,
        story.title.trim(),
        snapshot_json
    )
}

fn build_review_checklist_body(workflow: &ActiveWorkflowProjection) -> String {
    let primary_workflow = workflow
        .primary_workflow
        .as_ref()
        .map(|item| format!("{} (`{}`)", item.name, item.key))
        .unwrap_or_else(|| "(none)".to_string());
    let constraints = if workflow.effective_contract.constraints.is_empty() {
        "- (none)".to_string()
    } else {
        workflow
            .effective_contract
            .constraints
            .iter()
            .map(|constraint| {
                format!(
                    "- `{}` [{}] {}",
                    constraint.key,
                    enum_tag(&constraint.kind),
                    constraint.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let checks = if workflow.effective_contract.completion.checks.is_empty() {
        "- (none)".to_string()
    } else {
        workflow
            .effective_contract
            .completion
            .checks
            .iter()
            .map(|check| {
                format!(
                    "- `{}` [{}] {}",
                    check.key,
                    enum_tag(&check.kind),
                    check.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let default_artifact_type = workflow
        .effective_contract
        .completion
        .default_artifact_type
        .map(workflow_artifact_type_tag)
        .unwrap_or("-");
    let default_artifact_title = workflow
        .effective_contract
        .completion
        .default_artifact_title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("-");

    format!(
        "- lifecycle: {} (`{}`)\n- step: `{}`\n- primary_workflow: {}\n- default_artifact_type: `{}`\n- default_artifact_title: {}\n\n### Constraints\n{}\n\n### Completion Checks\n{}",
        workflow.lifecycle.name,
        workflow.lifecycle.key,
        workflow.active_step.key,
        primary_workflow,
        default_artifact_type,
        default_artifact_title,
        constraints,
        checks
    )
}

fn build_workspace_journal_body() -> String {
    "- journal_root: `.trellis/workspace/<developer>/journal-N.md`\n- guidance: 先通过 `python ./.trellis/scripts/get_context.py` 确认当前 developer 与 active journal，再在 record 阶段追加结构化记录。".to_string()
}

fn build_workflow_archive_action_body(
    task: &Task,
    workflow: &ActiveWorkflowProjection,
) -> String {
    format!(
        "- task_id: `{}`\n- active_step: `{}`\n- guidance: 当确认本次工作已收口后，可在 record 阶段产出 `archive_suggestion`，并使用 `python ./.trellis/scripts/task.py archive <task-name>` 归档对应 Trellis task 目录。",
        task.id,
        workflow.active_step.key
    )
}

fn enum_tag<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .map(|raw| raw.trim_matches('"').to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::context::{
        SessionEffectiveContext, SessionExecutorSummary, SessionOwnerContext, SessionProjectDefaults,
        SessionStoryOverrides,
    };
    use crate::session::plan::{SessionRuntimePolicySummary, SessionToolVisibilitySummary};
    use agentdash_domain::project::Project;
    use agentdash_domain::story::Story;
    use agentdash_domain::task::Task;
    use agentdash_domain::workflow::{
        EffectiveSessionContract, LifecycleDefinition, LifecycleRun, LifecycleStepDefinition,
        WorkflowBindingKind, WorkflowCheckKind, WorkflowCheckSpec, WorkflowCompletionSpec,
        WorkflowDefinition, WorkflowDefinitionSource, WorkflowInjectionSpec,
    };
    use uuid::Uuid;

    fn sample_execution_snapshot() -> SessionContextSnapshot {
        SessionContextSnapshot {
            executor: SessionExecutorSummary {
                executor: Some("PI_AGENT".to_string()),
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-5.4".to_string()),
                agent_id: None,
                thinking_level: None,
                permission_policy: Some("auto".to_string()),
                preset_name: Some("trellis".to_string()),
                source: "test".to_string(),
                resolution_error: None,
            },
            project_defaults: SessionProjectDefaults {
                default_agent_type: Some("PI_AGENT".to_string()),
                context_containers: vec![],
            },
            effective: SessionEffectiveContext {
                session_composition: Default::default(),
                tool_visibility: SessionToolVisibilitySummary {
                    markdown: "tools".to_string(),
                    resolved: true,
                    toolset_label: "address_space_runtime".to_string(),
                    tool_names: vec!["fs_read".to_string()],
                    mcp_servers: vec![],
                },
                runtime_policy: SessionRuntimePolicySummary {
                    markdown: "policy".to_string(),
                    workspace_attached: true,
                    address_space_attached: true,
                    mcp_enabled: false,
                    visible_mounts: vec!["main".to_string()],
                    visible_tools: vec!["fs_read".to_string()],
                    writable_mounts: vec!["`main`".to_string()],
                    exec_mounts: vec!["`main`".to_string()],
                    path_policy: "mount + relative path".to_string(),
                },
            },
            owner_context: SessionOwnerContext::Task {
                story_overrides: SessionStoryOverrides {
                    context_containers: vec![],
                    disabled_container_ids: vec![],
                    session_composition: None,
                },
            },
            session_capabilities: None,
        }
    }

    fn sample_workflow() -> ActiveWorkflowProjection {
        let definition = WorkflowDefinition::new(
            "wf_check",
            "Workflow Check",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            agentdash_domain::workflow::WorkflowContract {
                injection: WorkflowInjectionSpec::default(),
                completion: WorkflowCompletionSpec {
                    checks: vec![WorkflowCheckSpec {
                        key: "checklist_evidence_present".to_string(),
                        kind: WorkflowCheckKind::ChecklistEvidencePresent,
                        description: "必须产出 checklist evidence".to_string(),
                        payload: None,
                    }],
                    default_artifact_type: Some(
                        agentdash_domain::workflow::WorkflowRecordArtifactType::ChecklistEvidence,
                    ),
                    default_artifact_title: Some("检查证据".to_string()),
                },
                ..Default::default()
            },
        )
        .expect("workflow definition");
        let step = LifecycleStepDefinition {
            key: "check".to_string(),
            description: "执行 review".to_string(),
            workflow_key: Some(definition.key.clone()),
            node_type: Default::default(),
            depends_on: Vec::new(),
        };
        let lifecycle = LifecycleDefinition::new(
            "lc_task",
            "Lifecycle Task",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            "check",
            vec![step.clone()],
        )
        .expect("lifecycle definition");
        let binding_id = Uuid::new_v4();
        let run = LifecycleRun::new(
            Uuid::new_v4(),
            lifecycle.id,
            WorkflowBindingKind::Task,
            binding_id,
            &lifecycle.steps,
            &lifecycle.entry_step_key,
        )
        .expect("run");
        ActiveWorkflowProjection {
            run,
            lifecycle,
            active_step: step,
            primary_workflow: Some(definition.clone()),
            effective_contract: EffectiveSessionContract {
                lifecycle_key: Some("lc_task".to_string()),
                active_step_key: Some("check".to_string()),
                injection: definition.contract.injection.clone(),
                hook_rules: definition.contract.hook_rules.clone(),
                constraints: definition.contract.constraints.clone(),
                completion: definition.contract.completion.clone(),
            },
            binding: crate::workflow::WorkflowBindingSummary {
                binding_kind: WorkflowBindingKind::Task,
                binding_id,
                binding_label: Some("Task".to_string()),
            },
        }
    }

    #[test]
    fn task_runtime_context_entries_include_required_and_optional_keys() {
        let project = Project::new("Project".to_string(), "desc".to_string());
        let mut story = Story::new(project.id, "Story".to_string(), "story desc".to_string());
        story.context.prd_doc = Some("# PRD".to_string());
        let task = Task::new(project.id, story.id, "Task".to_string(), "task desc".to_string());
        let snapshot = sample_execution_snapshot();
        let workflow = sample_workflow();

        let entries = build_task_runtime_context_entries(
            &task,
            &story,
            &project,
            None,
            &snapshot,
            Some(&workflow),
        );

        assert!(entries.contains_key("execution_context"));
        assert!(entries.contains_key("review_checklist"));
        assert!(entries.contains_key("story_context_snapshot"));
        assert!(entries.contains_key("story_prd"));
        assert!(entries.contains_key("workspace_journal"));
        assert!(entries.contains_key("workflow_archive_action"));
    }
}
