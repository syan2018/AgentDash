use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{
    EffectiveSessionContract, LifecycleEdge, LifecycleExecutionEntry, LifecycleRunStatus,
    LifecycleStepDefinition, LifecycleStepExecutionStatus, LifecycleStepState, ValidationIssue,
    WorkflowBindingKind, WorkflowBindingRole, WorkflowContract,
    WorkflowDefinitionSource,
    node_deps_from_edges,
    validate_lifecycle_definition, validate_workflow_definition,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub source: WorkflowDefinitionSource,
    pub version: i32,
    pub contract: WorkflowContract,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        binding_kind: WorkflowBindingKind,
        source: WorkflowDefinitionSource,
        contract: WorkflowContract,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        validate_workflow_definition(&key, &name, &contract)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            binding_kind,
            recommended_binding_roles: Vec::new(),
            source,
            version: 1,
            contract,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_workflow_definition(&self.key, &self.name, &self.contract) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "workflow_definition_invalid",
                error,
                "contract",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub binding_kind: WorkflowBindingKind,
    #[serde(default)]
    pub recommended_binding_roles: Vec<WorkflowBindingRole>,
    pub source: WorkflowDefinitionSource,
    pub version: i32,
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinition>,
    /// Port-level DAG 边——唯一的拓扑数据源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<LifecycleEdge>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LifecycleDefinition {
    pub fn new(
        project_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        binding_kind: WorkflowBindingKind,
        source: WorkflowDefinitionSource,
        entry_step_key: impl Into<String>,
        steps: Vec<LifecycleStepDefinition>,
        edges: Vec<LifecycleEdge>,
    ) -> Result<Self, String> {
        let key = key.into();
        let name = name.into();
        let entry_step_key = entry_step_key.into();
        validate_lifecycle_definition(&key, &name, &entry_step_key, &steps, &edges)?;

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            key,
            name,
            description: description.into(),
            binding_kind,
            recommended_binding_roles: Vec::new(),
            source,
            version: 1,
            entry_step_key,
            steps,
            edges,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate_full(&self) -> Vec<ValidationIssue> {
        match validate_lifecycle_definition(
            &self.key,
            &self.name,
            &self.entry_step_key,
            &self.steps,
            &self.edges,
        ) {
            Ok(()) => Vec::new(),
            Err(error) => vec![ValidationIssue::error(
                "lifecycle_definition_invalid",
                error,
                "steps",
            )],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    /// 父 session ID — lifecycle run 跟着 session 走，不绑定 Task/Story。
    pub session_id: String,
    pub status: LifecycleRunStatus,
    /// 当前所有可执行（Ready/Running）的 node key 集合。
    /// 线性 lifecycle 中此集合只有 0 或 1 个元素。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_node_keys: Vec<String>,
    #[serde(default)]
    pub step_states: Vec<LifecycleStepState>,
    #[serde(default)]
    pub execution_log: Vec<LifecycleExecutionEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

impl LifecycleRun {
    /// 返回「当前活跃」的首个 step key。线性推进时即唯一活跃 step；
    /// DAG 下返回 `active_node_keys.first()`。
    pub fn current_step_key(&self) -> Option<&str> {
        self.active_node_keys.first().map(String::as_str)
    }
    pub fn new(
        project_id: Uuid,
        lifecycle_id: Uuid,
        session_id: impl Into<String>,
        steps: &[LifecycleStepDefinition],
        entry_step_key: &str,
        edges: &[LifecycleEdge],
    ) -> Result<Self, String> {
        if steps.is_empty() {
            return Err("lifecycle run 至少需要一个 step".to_string());
        }
        if !steps.iter().any(|step| step.key == entry_step_key) {
            return Err(format!("entry_step_key `{entry_step_key}` 不存在"));
        }

        let node_deps = node_deps_from_edges(edges);
        let has_edges = !edges.is_empty();

        let now = Utc::now();
        let step_states = steps
            .iter()
            .map(|step| {
                let status = if step.key == entry_step_key {
                    LifecycleStepExecutionStatus::Ready
                } else if has_edges && !node_deps.contains_key(step.key.as_str()) {
                    LifecycleStepExecutionStatus::Ready
                } else {
                    LifecycleStepExecutionStatus::Pending
                };
                LifecycleStepState {
                    step_key: step.key.clone(),
                    status,
                    session_id: None,
                    started_at: None,
                    completed_at: None,
                    summary: None,
                    context_snapshot: None,
                    gate_collision_count: 0,
                }
            })
            .collect::<Vec<_>>();

        let active_node_keys: Vec<String> = step_states
            .iter()
            .filter(|s| s.status == LifecycleStepExecutionStatus::Ready)
            .map(|s| s.step_key.clone())
            .collect();

        Ok(Self {
            id: Uuid::new_v4(),
            project_id,
            lifecycle_id,
            session_id: session_id.into(),
            status: LifecycleRunStatus::Ready,
            active_node_keys,
            step_states,
            execution_log: Vec::new(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        })
    }

    pub fn activate_step(&mut self, step_key: &str) -> Result<(), String> {
        let Some(index) = self
            .step_states
            .iter()
            .position(|step| step.step_key == step_key)
        else {
            return Err(format!("lifecycle run 不存在 step: {step_key}"));
        };

        if self.active_node_keys.is_empty() {
            return Err(format!("没有可激活的 step: {step_key}"));
        }
        if !self.active_node_keys.contains(&step_key.to_string()) {
            return Err(format!("step 不在当前可激活集合中: {step_key}"));
        }

        match self.step_states[index].status {
            LifecycleStepExecutionStatus::Ready => {}
            LifecycleStepExecutionStatus::Pending => {
                return Err(format!("step 尚未 ready: {step_key}"));
            }
            LifecycleStepExecutionStatus::Running => {
                return Err(format!("step 已在运行中: {step_key}"));
            }
            LifecycleStepExecutionStatus::Completed => {
                return Err(format!("step 已完成: {step_key}"));
            }
            LifecycleStepExecutionStatus::Failed => {
                return Err(format!("step 已失败，无法重新激活: {step_key}"));
            }
            LifecycleStepExecutionStatus::Skipped => {
                return Err(format!("step 已跳过，无法激活: {step_key}"));
            }
        }

        let now = Utc::now();
        self.status = LifecycleRunStatus::Running;
        self.step_states[index].status = LifecycleStepExecutionStatus::Running;
        self.step_states[index].started_at.get_or_insert(now);
        self.updated_at = now;
        self.last_activity_at = now;
        Ok(())
    }

    /// 完成指定 step 并计算后继 node 的就绪状态。
    ///
    /// 推进逻辑完全由 `edges` 驱动（flow + artifact 两类 edge 的 from_node
    /// 合并为 dependency set）。无出边的 step 即 terminal；所有 step 终态后 lifecycle 置 Completed。
    pub fn complete_step(
        &mut self,
        step_key: &str,
        summary: Option<String>,
        edges: &[LifecycleEdge],
    ) -> Result<(), String> {
        let Some(current_idx) = self
            .step_states
            .iter()
            .position(|step| step.step_key == step_key)
        else {
            return Err(format!("lifecycle run 不存在 step: {step_key}"));
        };

        if self.active_node_keys.is_empty() {
            return Err(format!("没有可完成的 step: {step_key}"));
        }
        if !self.active_node_keys.contains(&step_key.to_string()) {
            return Err(format!("step 不在当前可完成集合中: {step_key}"));
        }

        match self.step_states[current_idx].status {
            LifecycleStepExecutionStatus::Ready | LifecycleStepExecutionStatus::Running => {}
            LifecycleStepExecutionStatus::Pending => {
                return Err(format!("step 尚未 ready: {step_key}"));
            }
            LifecycleStepExecutionStatus::Completed => {
                return Err(format!("step 已完成: {step_key}"));
            }
            LifecycleStepExecutionStatus::Failed => {
                return Err(format!("step 已失败，无法直接完成: {step_key}"));
            }
            LifecycleStepExecutionStatus::Skipped => {
                return Err(format!("step 已跳过，无法完成: {step_key}"));
            }
        }

        let now = Utc::now();
        self.step_states[current_idx].started_at.get_or_insert(now);
        self.step_states[current_idx].status = LifecycleStepExecutionStatus::Completed;
        self.step_states[current_idx].completed_at = Some(now);
        self.step_states[current_idx].summary = summary;

        self.active_node_keys.retain(|k| k != step_key);

        self.advance_dag_successors(step_key, edges);

        if self.active_node_keys.is_empty() {
            let all_done = self.step_states.iter().all(|s| {
                matches!(
                    s.status,
                    LifecycleStepExecutionStatus::Completed
                        | LifecycleStepExecutionStatus::Skipped
                )
            });
            self.status = if all_done {
                LifecycleRunStatus::Completed
            } else {
                LifecycleRunStatus::Blocked
            };
        } else {
            self.status = LifecycleRunStatus::Ready;
        }

        self.updated_at = now;
        self.last_activity_at = now;
        Ok(())
    }

    /// DAG 后继解析：找出因当前 step 完成而变为 Ready 的 node（基于 edges）
    fn advance_dag_successors(&mut self, completed_key: &str, edges: &[LifecycleEdge]) {
        let completed_keys: std::collections::HashSet<&str> = self
            .step_states
            .iter()
            .filter(|s| s.status == LifecycleStepExecutionStatus::Completed)
            .map(|s| s.step_key.as_str())
            .collect();

        let node_deps = node_deps_from_edges(edges);

        let mut newly_ready: Vec<String> = Vec::new();
        for (to_node, from_nodes) in &node_deps {
            if !from_nodes.contains(completed_key) {
                continue;
            }
            let all_deps_met = from_nodes.iter().all(|dep| completed_keys.contains(dep));
            if !all_deps_met {
                continue;
            }
            let is_pending = self.step_states.iter().any(|s| {
                s.step_key == *to_node && s.status == LifecycleStepExecutionStatus::Pending
            });
            if is_pending {
                newly_ready.push(to_node.to_string());
            }
        }

        for key in &newly_ready {
            if let Some(state) = self.step_states.iter_mut().find(|s| s.step_key == *key) {
                state.status = LifecycleStepExecutionStatus::Ready;
            }
            if !self.active_node_keys.contains(key) {
                self.active_node_keys.push(key.clone());
            }
        }
    }

    pub fn append_execution_log(&mut self, entries: Vec<LifecycleExecutionEntry>) {
        if entries.is_empty() {
            return;
        }
        self.execution_log.extend(entries);
        self.updated_at = Utc::now();
        self.last_activity_at = self.updated_at;
    }
}

pub fn build_effective_contract(
    lifecycle_key: &str,
    active_step_key: &str,
    primary_workflow: Option<&WorkflowDefinition>,
) -> EffectiveSessionContract {
    match primary_workflow {
        Some(w) => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_step_key: Some(active_step_key.to_string()),
            injection: w.contract.injection.clone(),
            hook_rules: w.contract.hook_rules.clone(),
            constraints: w.contract.constraints.clone(),
            completion: w.contract.completion.clone(),
        },
        None => EffectiveSessionContract {
            lifecycle_key: Some(lifecycle_key.to_string()),
            active_step_key: Some(active_step_key.to_string()),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::value_objects::{
        WorkflowCompletionSpec, WorkflowConstraintKind, WorkflowConstraintSpec,
        WorkflowContextBinding, WorkflowInjectionSpec,
    };

    fn contract() -> WorkflowContract {
        WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions: vec!["follow the workflow".to_string()],
                context_bindings: vec![WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
                ..WorkflowInjectionSpec::default()
            },
            constraints: vec![WorkflowConstraintSpec {
                key: "c1".to_string(),
                kind: WorkflowConstraintKind::Custom,
                description: "constraint".to_string(),
                payload: None,
            }],
            completion: WorkflowCompletionSpec::default(),
            ..WorkflowContract::default()
        }
    }

    fn step(key: &str, workflow_key: &str) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: Some(workflow_key.to_string()),
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
        }
    }

    fn edge(from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> LifecycleEdge {
        LifecycleEdge::artifact(from_node, from_port, to_node, to_port)
    }

    fn flow_edge(from_node: &str, to_node: &str) -> LifecycleEdge {
        LifecycleEdge::flow(from_node, to_node)
    }

    #[test]
    fn lifecycle_run_completes_and_advances_linear() {
        let steps = [step("start", "wf_start"), step("check", "wf_check")];
        let edges = [flow_edge("start", "check")];
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "sess-test-linear",
            &steps,
            "start",
            &edges,
        )
        .expect("run");

        run.complete_step("start", Some("done".to_string()), &edges)
            .expect("complete");

        assert_eq!(run.current_step_key(), Some("check"));
        assert_eq!(run.status, LifecycleRunStatus::Ready);
    }

    #[test]
    fn lifecycle_run_single_step_completes_without_edges() {
        let steps = [step("solo", "wf_solo")];
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "sess-test-solo",
            &steps,
            "solo",
            &[],
        )
        .expect("run");

        run.complete_step("solo", Some("done".to_string()), &[])
            .expect("complete");

        assert!(run.active_node_keys.is_empty());
        assert_eq!(run.status, LifecycleRunStatus::Completed);
    }

    #[test]
    fn lifecycle_run_mixed_flow_and_artifact_edges() {
        let steps = [
            step("plan", "wf_plan"),
            step("build", "wf_build"),
            step("verify", "wf_verify"),
        ];
        // plan --flow--> build --artifact--> verify
        let edges = [
            flow_edge("plan", "build"),
            edge("build", "artifact", "verify", "input"),
        ];
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "sess-test-mixed",
            &steps,
            "plan",
            &edges,
        )
        .expect("run");

        assert_eq!(run.active_node_keys, vec!["plan".to_string()]);
        run.complete_step("plan", None, &edges).expect("plan done");
        assert!(run.active_node_keys.contains(&"build".to_string()));
        run.complete_step("build", None, &edges).expect("build done");
        assert!(run.active_node_keys.contains(&"verify".to_string()));
        run.complete_step("verify", None, &edges)
            .expect("verify done");
        assert_eq!(run.status, LifecycleRunStatus::Completed);
    }

    #[test]
    fn lifecycle_run_dag_all_complete_join() {
        let steps = [
            step("research", "wf_research"),
            step("analyze", "wf_analyze"),
            step("implement", "wf_impl"),
            step("check", "wf_check"),
        ];
        // research + analyze → implement → check
        let edges = [
            edge("research", "report", "implement", "research_input"),
            edge("analyze", "report", "implement", "analyze_input"),
            edge("implement", "code", "check", "code_input"),
        ];
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "sess-test-dag",
            &steps,
            "research",
            &edges,
        )
        .expect("run");

        // research 和 analyze 无入边 → 都应为 Ready
        assert_eq!(run.active_node_keys.len(), 2);
        assert!(run.active_node_keys.contains(&"research".to_string()));
        assert!(run.active_node_keys.contains(&"analyze".to_string()));

        // 完成 research，implement 还不应 Ready（analyze 未完成）
        run.complete_step("research", Some("done".to_string()), &edges)
            .expect("complete research");
        assert!(!run.active_node_keys.contains(&"implement".to_string()));
        assert!(run.active_node_keys.contains(&"analyze".to_string()));

        // 完成 analyze，implement 应变为 Ready（all-complete join）
        run.complete_step("analyze", Some("done".to_string()), &edges)
            .expect("complete analyze");
        assert!(run.active_node_keys.contains(&"implement".to_string()));
        assert_eq!(run.active_node_keys.len(), 1);

        // 完成 implement → check Ready
        run.complete_step("implement", Some("done".to_string()), &edges)
            .expect("complete implement");
        assert!(run.active_node_keys.contains(&"check".to_string()));

        // 完成 check → 全部完成
        run.complete_step("check", Some("done".to_string()), &edges)
            .expect("complete check");
        assert!(run.active_node_keys.is_empty());
        assert_eq!(run.status, LifecycleRunStatus::Completed);
        assert!(run.current_step_key().is_none());
    }

    #[test]
    fn effective_contract_matches_primary_workflow() {
        let primary = WorkflowDefinition::new(
            Uuid::new_v4(),
            "wf_primary",
            "Primary",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract(),
        )
        .expect("primary");

        let effective = build_effective_contract("lc", "step", Some(&primary));
        assert_eq!(effective.constraints.len(), 1);
    }

    #[test]
    fn lifecycle_definition_validates_step_graph() {
        let _lifecycle = LifecycleDefinition::new(
            Uuid::new_v4(),
            "lc",
            "Lifecycle",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            "start",
            vec![step("start", "wf_start")],
            vec![],
        )
        .expect("lifecycle");
    }
}
