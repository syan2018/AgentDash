use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::DomainError;
use crate::shared_library::InstalledAssetSource;

use super::validation::{validate_agent_procedure, validate_workflow_graph};
use super::value_objects::{
    ActivityDefinition, ActivityTransition, AgentProcedureContract, DefinitionSource,
    EffectiveSessionContract, LifecycleContext, LifecycleExecutionEntry, LifecycleRunStatus,
    LifecycleTaskPlanItem, LifecycleTaskPlanItemDraft, LifecycleTaskPlanItemPatch,
    OrchestrationInstance, OrchestrationStatus, RuntimeNodeState, RuntimeNodeStatus,
    TaskPlanStatus, ValidationIssue,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProcedure {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub source: DefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub version: i32,
    pub contract: AgentProcedureContract,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentProcedure {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        source: DefinitionSource,
        contract: AgentProcedureContract,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        validate_agent_procedure(&key, &name, &contract)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            source,
            installed_source: None,
            version: 1,
            contract,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_agent_procedure(&self.key, &self.name, &self.contract) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "agent_procedure_invalid",
                error,
                "contract",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGraph {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub source: DefinitionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub version: i32,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<ActivityTransition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WorkflowGraphDraft {
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub source: DefinitionSource,
    pub entry_activity_key: String,
    pub activities: Vec<ActivityDefinition>,
    pub transitions: Vec<ActivityTransition>,
}

impl WorkflowGraph {
    pub fn new(draft: WorkflowGraphDraft) -> Result<Self, String> {
        validate_workflow_graph(
            &draft.key,
            &draft.name,
            &draft.entry_activity_key,
            &draft.activities,
            &draft.transitions,
        )?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id: draft.project_id,
            key: draft.key,
            name: draft.name,
            description: draft.description,
            source: draft.source,
            installed_source: None,
            version: 1,
            entry_activity_key: draft.entry_activity_key,
            activities: draft.activities,
            transitions: draft.transitions,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_workflow_graph(
            &self.key,
            &self.name,
            &self.entry_activity_key,
            &self.activities,
            &self.transitions,
        ) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "workflow_graph_invalid",
                error,
                "activities",
            )],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunTopology {
    Graphless,
    WorkflowGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub topology: LifecycleRunTopology,
    #[serde(default)]
    pub context: LifecycleContext,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orchestrations: Vec<OrchestrationInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<LifecycleTaskPlanItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_projection: Option<Value>,
    pub status: LifecycleRunStatus,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    pub fn new_control(project_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            topology: LifecycleRunTopology::WorkflowGraph,
            context: LifecycleContext::default(),
            orchestrations: Vec::new(),
            tasks: Vec::new(),
            view_projection: None,
            status: LifecycleRunStatus::Ready,
            execution_log: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        }
    }

    pub fn new_graphless(project_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            topology: LifecycleRunTopology::Graphless,
            context: LifecycleContext::default(),
            orchestrations: Vec::new(),
            tasks: Vec::new(),
            view_projection: None,
            status: LifecycleRunStatus::Ready,
            execution_log: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        }
    }

    pub fn set_lifecycle_context(&mut self, context: LifecycleContext) {
        self.context = context;
        self.touch_activity();
    }

    pub fn add_orchestration(&mut self, orchestration: OrchestrationInstance) -> bool {
        let orchestration_id = orchestration.orchestration_id;
        if self.orchestration_by_id(orchestration_id).is_some() {
            return false;
        }
        self.orchestrations.push(orchestration);
        self.refresh_status_from_orchestrations();
        self.touch_activity();
        true
    }

    pub fn replace_orchestration(
        &mut self,
        orchestration: OrchestrationInstance,
    ) -> Option<OrchestrationInstance> {
        let orchestration_id = orchestration.orchestration_id;
        let position = self
            .orchestrations
            .iter()
            .position(|existing| existing.orchestration_id == orchestration_id)?;
        let previous = std::mem::replace(&mut self.orchestrations[position], orchestration);
        self.refresh_status_from_orchestrations();
        self.touch_activity();
        Some(previous)
    }

    pub fn orchestration_by_id(&self, orchestration_id: Uuid) -> Option<&OrchestrationInstance> {
        self.orchestrations
            .iter()
            .find(|orchestration| orchestration.orchestration_id == orchestration_id)
    }

    pub fn task_by_id(&self, task_id: Uuid) -> Option<&LifecycleTaskPlanItem> {
        self.tasks.iter().find(|task| task.id == task_id)
    }

    pub fn create_task(
        &mut self,
        draft: LifecycleTaskPlanItemDraft,
    ) -> Result<LifecycleTaskPlanItem, DomainError> {
        validate_task_title(&draft.title)?;
        validate_story_ref(&draft.story_ref)?;

        let task_id = draft.id.unwrap_or_else(Uuid::new_v4);
        if self.task_by_id(task_id).is_some() {
            return Err(DomainError::Conflict {
                entity: "LifecycleRun",
                constraint: "tasks.id",
                message: format!("Task plan item {task_id} already exists in run {}", self.id),
            });
        }

        let now = Utc::now();
        let task = LifecycleTaskPlanItem {
            id: task_id,
            title: draft.title,
            body: draft.body,
            status: draft.status,
            priority: draft.priority,
            created_by_agent_id: draft.created_by_agent_id,
            owner_agent_id: draft.owner_agent_id,
            assigned_agent_id: draft.assigned_agent_id,
            source_task_id: draft.source_task_id,
            created_at: now,
            updated_at: now,
            archived_at: None,
            context_refs: draft.context_refs,
            story_ref: draft.story_ref,
        };
        self.tasks.push(task.clone());
        self.touch_activity();
        Ok(task)
    }

    pub fn update_task(
        &mut self,
        task_id: Uuid,
        patch: LifecycleTaskPlanItemPatch,
    ) -> Result<LifecycleTaskPlanItem, DomainError> {
        if let Some(title) = patch.title.as_deref() {
            validate_task_title(title)?;
        }
        if let Some(story_ref) = &patch.story_ref {
            validate_story_ref(story_ref)?;
        }

        let task = self.task_by_id_mut(task_id)?;
        if let Some(title) = patch.title {
            task.title = title;
        }
        if let Some(body) = patch.body {
            task.body = body;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(owner_agent_id) = patch.owner_agent_id {
            task.owner_agent_id = owner_agent_id;
        }
        if let Some(assigned_agent_id) = patch.assigned_agent_id {
            task.assigned_agent_id = assigned_agent_id;
        }
        if let Some(source_task_id) = patch.source_task_id {
            task.source_task_id = source_task_id;
        }
        if let Some(context_refs) = patch.context_refs {
            task.context_refs = context_refs;
        }
        if let Some(story_ref) = patch.story_ref {
            task.story_ref = story_ref;
        }
        task.updated_at = Utc::now();
        let updated = task.clone();
        self.touch_activity();
        Ok(updated)
    }

    pub fn archive_task(&mut self, task_id: Uuid) -> Result<LifecycleTaskPlanItem, DomainError> {
        let task = self.task_by_id_mut(task_id)?;
        let now = Utc::now();
        if task.archived_at.is_none() {
            task.archived_at = Some(now);
            task.status = TaskPlanStatus::Dropped;
            task.updated_at = now;
        }
        let archived = task.clone();
        self.touch_activity();
        Ok(archived)
    }

    pub fn transition_task_status(
        &mut self,
        task_id: Uuid,
        next: TaskPlanStatus,
    ) -> Result<LifecycleTaskPlanItem, DomainError> {
        let task = self.task_by_id_mut(task_id)?;
        if !task.status.can_transition_to(next) {
            return Err(DomainError::InvalidTransition {
                from: format!("{:?}", task.status),
                to: format!("{next:?}"),
            });
        }
        if task.status != next {
            task.status = next;
            task.updated_at = Utc::now();
        }
        let transitioned = task.clone();
        self.touch_activity();
        Ok(transitioned)
    }

    pub fn reorder_tasks(
        &mut self,
        ordered_task_ids: &[Uuid],
    ) -> Result<Vec<LifecycleTaskPlanItem>, DomainError> {
        for task_id in ordered_task_ids {
            if self.task_by_id(*task_id).is_none() {
                return Err(DomainError::NotFound {
                    entity: "LifecycleRun.tasks",
                    id: task_id.to_string(),
                });
            }
        }

        let now = Utc::now();
        let mut reordered = Vec::with_capacity(self.tasks.len());
        for task_id in ordered_task_ids {
            let mut task = self.task_by_id(*task_id).expect("validated task id").clone();
            task.updated_at = now;
            reordered.push(task);
        }
        for task in &self.tasks {
            if !ordered_task_ids.contains(&task.id) {
                reordered.push(task.clone());
            }
        }
        self.tasks = reordered;
        self.touch_activity();
        Ok(self.tasks.clone())
    }

    pub fn append_execution_log(&mut self, entries: Vec<LifecycleExecutionEntry>) {
        if entries.is_empty() {
            return;
        }
        self.execution_log.extend(entries);
        self.touch_activity();
    }

    fn touch_activity(&mut self) {
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
    }

    fn task_by_id_mut(&mut self, task_id: Uuid) -> Result<&mut LifecycleTaskPlanItem, DomainError> {
        self.tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "LifecycleRun.tasks",
                id: task_id.to_string(),
            })
    }

    pub fn refresh_status_from_orchestrations(&mut self) {
        self.status = aggregate_lifecycle_run_status(&self.orchestrations);
    }
}

fn validate_task_title(title: &str) -> Result<(), DomainError> {
    if title.trim().is_empty() {
        return Err(DomainError::InvalidConfig(
            "lifecycle_run.tasks.title 不能为空".to_string(),
        ));
    }
    Ok(())
}

fn validate_story_ref(
    story_ref: &Option<super::lifecycle_subject_association::SubjectRef>,
) -> Result<(), DomainError> {
    if let Some(story_ref) = story_ref {
        if story_ref.kind != "story" {
            return Err(DomainError::InvalidConfig(format!(
                "lifecycle_run.tasks.story_ref.kind 必须是 story，实际为 {}",
                story_ref.kind
            )));
        }
    }
    Ok(())
}

pub fn aggregate_lifecycle_run_status(
    orchestrations: &[OrchestrationInstance],
) -> LifecycleRunStatus {
    if orchestrations.is_empty() {
        return LifecycleRunStatus::Ready;
    }
    if orchestrations
        .iter()
        .any(|instance| instance.status == OrchestrationStatus::Failed)
    {
        return LifecycleRunStatus::Failed;
    }
    if orchestrations
        .iter()
        .any(|instance| instance.status == OrchestrationStatus::Paused)
        || orchestration_nodes(orchestrations)
            .iter()
            .any(|node| node.status == RuntimeNodeStatus::Blocked)
    {
        return LifecycleRunStatus::Blocked;
    }
    if orchestrations
        .iter()
        .any(|instance| instance.status == OrchestrationStatus::Running)
        || orchestration_nodes(orchestrations).iter().any(|node| {
            matches!(
                node.status,
                RuntimeNodeStatus::Ready | RuntimeNodeStatus::Claiming | RuntimeNodeStatus::Running
            )
        })
    {
        return LifecycleRunStatus::Running;
    }
    if orchestrations
        .iter()
        .any(|instance| instance.status == OrchestrationStatus::Pending)
    {
        return LifecycleRunStatus::Ready;
    }
    if orchestrations
        .iter()
        .all(|instance| instance.status == OrchestrationStatus::Cancelled)
    {
        return LifecycleRunStatus::Cancelled;
    }
    if orchestrations.iter().all(|instance| {
        matches!(
            instance.status,
            OrchestrationStatus::Completed | OrchestrationStatus::Cancelled
        )
    }) {
        return LifecycleRunStatus::Completed;
    }
    LifecycleRunStatus::Ready
}

fn orchestration_nodes(orchestrations: &[OrchestrationInstance]) -> Vec<&RuntimeNodeState> {
    fn collect<'a>(node: &'a RuntimeNodeState, nodes: &mut Vec<&'a RuntimeNodeState>) {
        nodes.push(node);
        for child in &node.children {
            collect(child, nodes);
        }
    }

    let mut nodes = Vec::new();
    for instance in orchestrations {
        for node in &instance.node_tree {
            collect(node, &mut nodes);
        }
    }
    nodes
}

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_activity_key: &str,
    primary_workflow: Option<&AgentProcedure>,
) -> EffectiveSessionContract {
    match primary_workflow {
        Some(w) => {
            build_effective_contract_from_contract(lifecycle_key, active_activity_key, &w.contract)
        }
        None => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_activity_key: Some(active_activity_key.to_string()),
            ..Default::default()
        },
    }
}

pub fn build_effective_contract_from_contract(
    lifecycle_key: &str,
    active_activity_key: &str,
    contract: &AgentProcedureContract,
) -> EffectiveSessionContract {
    EffectiveSessionContract {
        lifecycle_key: Some(lifecycle_key.to_string()),
        active_activity_key: Some(active_activity_key.to_string()),
        injection: contract.injection.clone(),
        hook_rules: contract.hook_rules.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::TaskPriority;
    use crate::workflow::value_objects::{
        AgentProcedureExecutionSpec, AgentReusePolicy, BashExecExecutorSpec, ExecutorSpec,
        FunctionActivityExecutorSpec, HumanActivityExecutorSpec, HumanApprovalExecutorSpec,
        OrchestrationPlanSnapshot, OrchestrationSourceRef, OrchestrationStatus, PlanNode,
        PlanNodeKind, RuntimeSessionPolicy, WorkflowContextBinding, WorkflowInjectionSpec,
    };

    fn contract() -> AgentProcedureContract {
        AgentProcedureContract {
            injection: WorkflowInjectionSpec {
                guidance: Some("follow the workflow".to_string()),
                context_bindings: vec![WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
            },
            ..AgentProcedureContract::default()
        }
    }

    #[test]
    fn effective_contract_matches_primary_workflow() {
        let primary = AgentProcedure::new(
            Uuid::new_v4(),
            "wf_primary",
            "Primary",
            "desc",
            DefinitionSource::BuiltinSeed,
            contract(),
        )
        .expect("primary");

        let effective = build_effective_contract("lc", "step", Some(&primary));
        assert_eq!(effective.hook_rules.len(), 0);
    }

    fn orchestration_instance(role: &str, executor: ExecutorSpec) -> OrchestrationInstance {
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id: Uuid::new_v4(),
            graph_version: Some(1),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: format!("sha256:{role}"),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: role.to_string(),
                node_path: role.to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::Activity,
                label: None,
                executor: Some(executor),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec![role.to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        OrchestrationInstance::new(role, source_ref, plan_snapshot)
    }

    fn agent_executor() -> ExecutorSpec {
        ExecutorSpec::AgentProcedure {
            procedure: AgentProcedureExecutionSpec::by_key("workflow.plan"),
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::CreateNew,
        }
    }

    fn function_executor() -> ExecutorSpec {
        ExecutorSpec::Function {
            spec: FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
                command: "pnpm".to_string(),
                args: vec!["test".to_string()],
                working_directory: None,
            }),
        }
    }

    fn human_executor() -> ExecutorSpec {
        ExecutorSpec::Human {
            spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                form_schema_key: "approval.plan_review".to_string(),
                title: None,
            }),
        }
    }

    #[test]
    fn lifecycle_run_orchestration_contract_defaults_empty() {
        let control = LifecycleRun::new_control(Uuid::new_v4());
        assert_eq!(control.context, LifecycleContext::default());
        assert!(control.orchestrations.is_empty());
        assert!(control.tasks.is_empty());
        assert!(control.view_projection.is_none());

        let graphless = LifecycleRun::new_graphless(Uuid::new_v4());
        assert_eq!(graphless.context, LifecycleContext::default());
        assert!(graphless.orchestrations.is_empty());
        assert!(graphless.tasks.is_empty());
        assert!(graphless.view_projection.is_none());
    }

    #[test]
    fn lifecycle_run_task_plan_create_writes_plan_facts_only() {
        let mut run = LifecycleRun::new_graphless(Uuid::new_v4());
        let creator_agent_id = Uuid::new_v4();
        let owner_agent_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let mut draft = LifecycleTaskPlanItemDraft::new("Implement task facts");
        draft.body = Some("Add aggregate-owned plan item state".to_string());
        draft.priority = Some(TaskPriority::P1);
        draft.created_by_agent_id = Some(creator_agent_id);
        draft.owner_agent_id = Some(owner_agent_id);
        draft.story_ref =
            Some(super::super::lifecycle_subject_association::SubjectRef::new("story", story_id));

        let created = run.create_task(draft).expect("create task");

        assert_eq!(run.tasks.len(), 1);
        assert_eq!(created.status, TaskPlanStatus::Open);
        assert_eq!(created.created_by_agent_id, Some(creator_agent_id));
        assert_eq!(created.owner_agent_id, Some(owner_agent_id));
        assert_eq!(
            created.story_ref.as_ref().map(|subject| subject.id),
            Some(story_id)
        );

        let value = serde_json::to_value(&created).expect("serialize task");
        assert!(value.get("dispatch_preference").is_none());
        assert!(value.get("artifacts").is_none());
        assert!(value.get("execution_status").is_none());
        assert!(value.get("runtime_status").is_none());
    }

    #[test]
    fn lifecycle_run_task_plan_update_changes_editable_plan_fields() {
        let mut run = LifecycleRun::new_graphless(Uuid::new_v4());
        let created = run
            .create_task(LifecycleTaskPlanItemDraft::new("Initial title"))
            .expect("create task");
        let owner_agent_id = Uuid::new_v4();
        let assigned_agent_id = Uuid::new_v4();

        let updated = run
            .update_task(
                created.id,
                LifecycleTaskPlanItemPatch {
                    title: Some("Updated title".to_string()),
                    body: Some(Some("Updated body".to_string())),
                    priority: Some(Some(TaskPriority::P0)),
                    owner_agent_id: Some(Some(owner_agent_id)),
                    assigned_agent_id: Some(Some(assigned_agent_id)),
                    ..LifecycleTaskPlanItemPatch::default()
                },
            )
            .expect("update task");

        assert_eq!(updated.title, "Updated title");
        assert_eq!(updated.body.as_deref(), Some("Updated body"));
        assert_eq!(updated.priority, Some(TaskPriority::P0));
        assert_eq!(updated.owner_agent_id, Some(owner_agent_id));
        assert_eq!(updated.assigned_agent_id, Some(assigned_agent_id));
        assert_eq!(updated.status, TaskPlanStatus::Open);
        assert!(updated.updated_at >= updated.created_at);
    }

    #[test]
    fn lifecycle_run_task_plan_archive_marks_dropped_and_archived() {
        let mut run = LifecycleRun::new_graphless(Uuid::new_v4());
        let task = run
            .create_task(LifecycleTaskPlanItemDraft::new("Archive me"))
            .expect("create task");

        let archived = run.archive_task(task.id).expect("archive task");

        assert_eq!(archived.status, TaskPlanStatus::Dropped);
        assert!(archived.archived_at.is_some());
        assert_eq!(
            run.task_by_id(task.id).expect("task").status,
            TaskPlanStatus::Dropped
        );
    }

    #[test]
    fn lifecycle_run_task_plan_status_transition_enforces_plan_language() {
        let mut run = LifecycleRun::new_graphless(Uuid::new_v4());
        let task = run
            .create_task(LifecycleTaskPlanItemDraft::new("Transition me"))
            .expect("create task");

        let active = run
            .transition_task_status(task.id, TaskPlanStatus::Active)
            .expect("open to active");
        assert_eq!(active.status, TaskPlanStatus::Active);

        let blocked = run
            .transition_task_status(task.id, TaskPlanStatus::Blocked)
            .expect("active to blocked");
        assert_eq!(blocked.status, TaskPlanStatus::Blocked);

        let active = run
            .transition_task_status(task.id, TaskPlanStatus::Active)
            .expect("blocked to active");
        assert_eq!(active.status, TaskPlanStatus::Active);

        let done = run
            .transition_task_status(task.id, TaskPlanStatus::Done)
            .expect("active to done");
        assert_eq!(done.status, TaskPlanStatus::Done);

        let err = run
            .transition_task_status(task.id, TaskPlanStatus::Review)
            .expect_err("done cannot move to review");
        assert!(matches!(err, DomainError::InvalidTransition { .. }));
    }

    #[test]
    fn lifecycle_run_orchestration_aggregate_adds_replaces_and_finds_one_instance() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        let context = LifecycleContext {
            main_agent_run_id: Some(Uuid::new_v4()),
            ..LifecycleContext::default()
        };
        run.set_lifecycle_context(context.clone());
        assert_eq!(run.context, context);

        let orchestration = orchestration_instance("root", agent_executor());
        let orchestration_id = orchestration.orchestration_id;
        assert!(run.add_orchestration(orchestration.clone()));
        assert!(!run.add_orchestration(orchestration));
        assert_eq!(
            run.orchestration_by_id(orchestration_id)
                .expect("orchestration")
                .role,
            "root"
        );

        let mut replacement = orchestration_instance("root_replacement", function_executor());
        replacement.orchestration_id = orchestration_id;
        replacement.status = OrchestrationStatus::Running;
        let previous = run
            .replace_orchestration(replacement)
            .expect("previous orchestration");
        assert_eq!(previous.role, "root");
        assert_eq!(
            run.orchestration_by_id(orchestration_id)
                .expect("orchestration")
                .status,
            OrchestrationStatus::Running
        );
    }

    #[test]
    fn lifecycle_run_orchestration_aggregate_keeps_multiple_executor_instances() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        assert!(run.add_orchestration(orchestration_instance("agent", agent_executor())));
        assert!(run.add_orchestration(orchestration_instance("function", function_executor())));
        assert!(run.add_orchestration(orchestration_instance("human", human_executor())));

        assert_eq!(run.orchestrations.len(), 3);
        assert!(matches!(
            run.orchestrations[0].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("agent executor"),
            ExecutorSpec::AgentProcedure { .. }
        ));
        assert!(matches!(
            run.orchestrations[1].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("function executor"),
            ExecutorSpec::Function { .. }
        ));
        assert!(matches!(
            run.orchestrations[2].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("human executor"),
            ExecutorSpec::Human { .. }
        ));
    }

    #[test]
    fn lifecycle_run_status_aggregates_mixed_terminal_as_completed() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        let mut completed = orchestration_instance("completed", agent_executor());
        completed.status = OrchestrationStatus::Completed;
        let mut cancelled = orchestration_instance("cancelled", function_executor());
        cancelled.status = OrchestrationStatus::Cancelled;

        assert!(run.add_orchestration(completed));
        assert!(run.add_orchestration(cancelled));

        assert_eq!(run.status, LifecycleRunStatus::Completed);
    }

    #[test]
    fn lifecycle_run_status_aggregates_paused_as_blocked() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        let mut paused = orchestration_instance("paused", human_executor());
        paused.status = OrchestrationStatus::Paused;

        assert!(run.add_orchestration(paused));

        assert_eq!(run.status, LifecycleRunStatus::Blocked);
    }

    #[test]
    fn lifecycle_run_status_aggregates_all_cancelled_as_cancelled() {
        let mut run = LifecycleRun::new_control(Uuid::new_v4());
        let mut first = orchestration_instance("cancelled_a", agent_executor());
        first.status = OrchestrationStatus::Cancelled;
        let mut second = orchestration_instance("cancelled_b", function_executor());
        second.status = OrchestrationStatus::Cancelled;

        assert!(run.add_orchestration(first));
        assert!(run.add_orchestration(second));

        assert_eq!(run.status, LifecycleRunStatus::Cancelled);
    }
}
