use uuid::Uuid;

use agentdash_domain::workflow::{
    WorkflowDefinition, WorkflowDefinitionRepository, WorkflowPhaseDefinition,
    WorkflowRecordArtifact, WorkflowRecordArtifactType, WorkflowRun, WorkflowRunRepository,
    WorkflowRunStatus, WorkflowTargetKind,
};

use super::error::WorkflowApplicationError;
use super::completion::WorkflowCompletionDecision;

#[derive(Debug, Clone)]
pub struct StartWorkflowRunCommand {
    pub workflow_id: Option<Uuid>,
    pub workflow_key: Option<String>,
    pub target_kind: WorkflowTargetKind,
    pub target_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ActivateWorkflowPhaseCommand {
    pub run_id: Uuid,
    pub phase_key: String,
    pub session_binding_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct CompleteWorkflowPhaseCommand {
    pub run_id: Uuid,
    pub phase_key: String,
    pub summary: Option<String>,
    pub record_artifacts: Vec<WorkflowRecordArtifactDraft>,
}

#[derive(Debug, Clone)]
pub struct AppendWorkflowPhaseArtifactsCommand {
    pub run_id: Uuid,
    pub phase_key: String,
    pub artifacts: Vec<WorkflowRecordArtifactDraft>,
}

#[derive(Debug, Clone)]
pub struct WorkflowRecordArtifactDraft {
    pub artifact_type: WorkflowRecordArtifactType,
    pub title: String,
    pub content: String,
}

impl WorkflowRecordArtifactDraft {
    fn into_artifact(self, phase_key: &str) -> WorkflowRecordArtifact {
        WorkflowRecordArtifact::new(phase_key, self.artifact_type, self.title, self.content)
    }
}

pub fn build_phase_completion_artifact_drafts(
    phase_key: &str,
    default_artifact_type: Option<WorkflowRecordArtifactType>,
    default_artifact_title: Option<&str>,
    decision: &WorkflowCompletionDecision,
) -> Vec<WorkflowRecordArtifactDraft> {
    let title = default_artifact_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{phase_key} 阶段记录"));
    let artifact_type = default_artifact_type.unwrap_or(WorkflowRecordArtifactType::PhaseNote);

    let mut sections = Vec::new();
    if let Some(summary) = decision.summary.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        sections.push(format!("## 阶段总结\n{summary}"));
    }
    if !decision.evidence.is_empty() {
        sections.push(format!(
            "## 完成证据\n{}",
            decision
                .evidence
                .iter()
                .map(|entry| {
                    let detail = entry
                        .detail
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|detail| format!(" ({detail})"))
                        .unwrap_or_default();
                    format!("- {}: {}{}", entry.code, entry.summary, detail)
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if sections.is_empty() {
        return Vec::new();
    }

    vec![WorkflowRecordArtifactDraft {
        artifact_type,
        title,
        content: sections.join("\n\n"),
    }]
}

pub struct WorkflowRunService<'a, D: ?Sized, R: ?Sized> {
    definition_repo: &'a D,
    run_repo: &'a R,
}

impl<'a, D: ?Sized, R: ?Sized> WorkflowRunService<'a, D, R>
where
    D: WorkflowDefinitionRepository,
    R: WorkflowRunRepository,
{
    pub fn new(definition_repo: &'a D, run_repo: &'a R) -> Self {
        Self {
            definition_repo,
            run_repo,
        }
    }

    pub async fn start_run(
        &self,
        cmd: StartWorkflowRunCommand,
    ) -> Result<WorkflowRun, WorkflowApplicationError> {
        let definition = self.resolve_definition(&cmd).await?;

        if !definition.enabled {
            return Err(WorkflowApplicationError::Conflict(format!(
                "workflow `{}` 已停用，不能启动 run",
                definition.key
            )));
        }

        if definition.target_kind != cmd.target_kind {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "workflow `{}` 仅支持 target_kind={:?}，收到 {:?}",
                definition.key, definition.target_kind, cmd.target_kind
            )));
        }

        let existing_runs = self
            .run_repo
            .list_by_target(cmd.target_kind, cmd.target_id)
            .await?;
        if existing_runs.iter().any(|run| {
            run.workflow_id == definition.id
                && matches!(
                    run.status,
                    WorkflowRunStatus::Ready
                        | WorkflowRunStatus::Running
                        | WorkflowRunStatus::Blocked
                )
        }) {
            return Err(WorkflowApplicationError::Conflict(format!(
                "目标对象 {} 已存在进行中的 workflow run",
                cmd.target_id
            )));
        }

        let run = WorkflowRun::new(
            definition.id,
            cmd.target_kind,
            cmd.target_id,
            &definition.phases,
        );
        self.run_repo.create(&run).await?;
        Ok(run)
    }

    pub async fn activate_phase(
        &self,
        cmd: ActivateWorkflowPhaseCommand,
    ) -> Result<WorkflowRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        let definition = self.load_definition(run.workflow_id).await?;
        let phase_definition = find_phase_definition(&definition, &cmd.phase_key)?;
        let existing_binding = run
            .phase_states
            .iter()
            .find(|phase| phase.phase_key == cmd.phase_key)
            .and_then(|phase| phase.session_binding_id);

        if phase_definition.requires_session
            && existing_binding.is_none()
            && cmd.session_binding_id.is_none()
        {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "phase `{}` 需要 session_binding_id",
                cmd.phase_key
            )));
        }

        if let Some(session_binding_id) = cmd.session_binding_id {
            run.attach_session_binding(&cmd.phase_key, session_binding_id)
                .map_err(WorkflowApplicationError::Conflict)?;
        }

        run.activate_phase(&cmd.phase_key)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        Ok(run)
    }

    pub async fn complete_phase(
        &self,
        cmd: CompleteWorkflowPhaseCommand,
    ) -> Result<WorkflowRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        let definition = self.load_definition(run.workflow_id).await?;
        let phase_definition = find_phase_definition(&definition, &cmd.phase_key)?;
        let phase_state = run
            .phase_states
            .iter()
            .find(|phase| phase.phase_key == cmd.phase_key)
            .cloned()
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow run phase 不存在: {}",
                    cmd.phase_key
                ))
            })?;

        if phase_definition.requires_session && phase_state.session_binding_id.is_none() {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "phase `{}` 需要先绑定 session",
                cmd.phase_key
            )));
        }

        run.complete_phase(&cmd.phase_key, cmd.summary)
            .map_err(WorkflowApplicationError::Conflict)?;
        for artifact in cmd.record_artifacts {
            run.append_record_artifact(artifact.into_artifact(&cmd.phase_key));
        }

        self.run_repo.update(&run).await?;
        Ok(run)
    }

    pub async fn append_phase_artifacts(
        &self,
        cmd: AppendWorkflowPhaseArtifactsCommand,
    ) -> Result<WorkflowRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        let definition = self.load_definition(run.workflow_id).await?;
        let _phase_definition = find_phase_definition(&definition, &cmd.phase_key)?;
        let phase_state = run
            .phase_states
            .iter()
            .find(|phase| phase.phase_key == cmd.phase_key)
            .cloned()
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow run phase 不存在: {}",
                    cmd.phase_key
                ))
            })?;

        if run.current_phase_key.as_deref() != Some(cmd.phase_key.as_str()) {
            return Err(WorkflowApplicationError::Conflict(format!(
                "当前活跃 phase 不是 `{}`，不能向其追加记录产物",
                cmd.phase_key
            )));
        }
        if !matches!(
            phase_state.status,
            agentdash_domain::workflow::WorkflowPhaseExecutionStatus::Ready
                | agentdash_domain::workflow::WorkflowPhaseExecutionStatus::Running
        ) {
            return Err(WorkflowApplicationError::Conflict(format!(
                "phase `{}` 当前状态为 {:?}，不能追加记录产物",
                cmd.phase_key, phase_state.status
            )));
        }

        for artifact in cmd.artifacts {
            run.append_record_artifact(artifact.into_artifact(&cmd.phase_key));
        }

        self.run_repo.update(&run).await?;
        Ok(run)
    }

    async fn resolve_definition(
        &self,
        cmd: &StartWorkflowRunCommand,
    ) -> Result<WorkflowDefinition, WorkflowApplicationError> {
        match (&cmd.workflow_id, &cmd.workflow_key) {
            (Some(_), Some(_)) => Err(WorkflowApplicationError::BadRequest(
                "workflow_id 与 workflow_key 只能提供一个".to_string(),
            )),
            (None, None) => Err(WorkflowApplicationError::BadRequest(
                "必须提供 workflow_id 或 workflow_key".to_string(),
            )),
            (Some(workflow_id), None) => self.load_definition(*workflow_id).await,
            (None, Some(workflow_key)) => self
                .definition_repo
                .get_by_key(workflow_key)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "workflow_definition 不存在: {}",
                        workflow_key
                    ))
                }),
        }
    }

    async fn load_definition(
        &self,
        workflow_id: Uuid,
    ) -> Result<WorkflowDefinition, WorkflowApplicationError> {
        self.definition_repo
            .get_by_id(workflow_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow_definition 不存在: {}",
                    workflow_id
                ))
            })
    }

    async fn load_run(&self, run_id: Uuid) -> Result<WorkflowRun, WorkflowApplicationError> {
        self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!("workflow_run 不存在: {}", run_id))
        })
    }
}

fn find_phase_definition<'a>(
    definition: &'a WorkflowDefinition,
    phase_key: &str,
) -> Result<&'a WorkflowPhaseDefinition, WorkflowApplicationError> {
    definition
        .phases
        .iter()
        .find(|phase| phase.key == phase_key)
        .ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!(
                "workflow_definition `{}` 不存在 phase `{}`",
                definition.key, phase_key
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        WorkflowDefinition, WorkflowDefinitionRepository, WorkflowRun, WorkflowRunRepository,
    };

    use super::*;
    use crate::workflow::{TRELLIS_DEV_TASK_TEMPLATE_KEY, build_builtin_workflow_definition};

    #[derive(Default)]
    struct MemoryWorkflowRunStore {
        definitions: Mutex<HashMap<Uuid, WorkflowDefinition>>,
        runs: Mutex<HashMap<Uuid, WorkflowRun>>,
    }

    #[async_trait]
    impl WorkflowDefinitionRepository for MemoryWorkflowRunStore {
        async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            self.definitions
                .lock()
                .expect("lock")
                .insert(workflow.id, workflow.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self.definitions.lock().expect("lock").get(&id).cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .find(|workflow| workflow.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .cloned()
                .collect())
        }

        async fn list_enabled(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .filter(|workflow| workflow.enabled)
                .cloned()
                .collect())
        }

        async fn list_by_target_kind(
            &self,
            target_kind: WorkflowTargetKind,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .filter(|workflow| workflow.target_kind == target_kind)
                .cloned()
                .collect())
        }

        async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            self.definitions
                .lock()
                .expect("lock")
                .insert(workflow.id, workflow.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.definitions.lock().expect("lock").remove(&id);
            Ok(())
        }
    }

    #[async_trait]
    impl WorkflowRunRepository for MemoryWorkflowRunStore {
        async fn create(&self, run: &WorkflowRun) -> Result<(), DomainError> {
            self.runs.lock().expect("lock").insert(run.id, run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowRun>, DomainError> {
            Ok(self.runs.lock().expect("lock").get(&id).cloned())
        }

        async fn list_by_workflow(
            &self,
            workflow_id: Uuid,
        ) -> Result<Vec<WorkflowRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .expect("lock")
                .values()
                .filter(|run| run.workflow_id == workflow_id)
                .cloned()
                .collect())
        }

        async fn list_by_target(
            &self,
            target_kind: WorkflowTargetKind,
            target_id: Uuid,
        ) -> Result<Vec<WorkflowRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .expect("lock")
                .values()
                .filter(|run| run.target_kind == target_kind && run.target_id == target_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &WorkflowRun) -> Result<(), DomainError> {
            self.runs.lock().expect("lock").insert(run.id, run.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().expect("lock").remove(&id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn start_run_by_workflow_key_creates_ready_run() {
        let store = MemoryWorkflowRunStore::default();
        let definition =
            build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("definition");
        WorkflowDefinitionRepository::create(&store, &definition)
            .await
            .expect("store definition");

        let service = WorkflowRunService::new(&store, &store);
        let run = service
            .start_run(StartWorkflowRunCommand {
                workflow_id: None,
                workflow_key: Some(TRELLIS_DEV_TASK_TEMPLATE_KEY.to_string()),
                target_kind: WorkflowTargetKind::Task,
                target_id: Uuid::new_v4(),
            })
            .await
            .expect("start run");

        assert_eq!(run.workflow_id, definition.id);
        assert_eq!(run.current_phase_key.as_deref(), Some("start"));
        assert_eq!(run.status, WorkflowRunStatus::Ready);
    }

    #[tokio::test]
    async fn start_run_rejects_duplicate_active_run_for_same_target() {
        let store = MemoryWorkflowRunStore::default();
        let definition =
            build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("definition");
        WorkflowDefinitionRepository::create(&store, &definition)
            .await
            .expect("store definition");

        let service = WorkflowRunService::new(&store, &store);
        let target_id = Uuid::new_v4();
        service
            .start_run(StartWorkflowRunCommand {
                workflow_id: Some(definition.id),
                workflow_key: None,
                target_kind: WorkflowTargetKind::Task,
                target_id,
            })
            .await
            .expect("first run");

        let error = service
            .start_run(StartWorkflowRunCommand {
                workflow_id: Some(definition.id),
                workflow_key: None,
                target_kind: WorkflowTargetKind::Task,
                target_id,
            })
            .await
            .expect_err("should reject duplicate run");

        assert!(error.to_string().contains("进行中的 workflow run"));
    }

    #[tokio::test]
    async fn activate_phase_requires_session_binding_for_session_phase() {
        let store = MemoryWorkflowRunStore::default();
        let definition =
            build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("definition");
        WorkflowDefinitionRepository::create(&store, &definition)
            .await
            .expect("store definition");

        let service = WorkflowRunService::new(&store, &store);
        let run = service
            .start_run(StartWorkflowRunCommand {
                workflow_id: Some(definition.id),
                workflow_key: None,
                target_kind: WorkflowTargetKind::Task,
                target_id: Uuid::new_v4(),
            })
            .await
            .expect("start run");
        let run = service
            .complete_phase(CompleteWorkflowPhaseCommand {
                run_id: run.id,
                phase_key: "start".to_string(),
                summary: Some("done".to_string()),
                record_artifacts: vec![],
            })
            .await
            .expect("complete start");

        let error = service
            .activate_phase(ActivateWorkflowPhaseCommand {
                run_id: run.id,
                phase_key: "implement".to_string(),
                session_binding_id: None,
            })
            .await
            .expect_err("should require session binding");

        assert!(error.to_string().contains("需要 session_binding_id"));
    }

    #[tokio::test]
    async fn complete_phase_persists_record_artifacts_and_advances() {
        let store = MemoryWorkflowRunStore::default();
        let definition =
            build_builtin_workflow_definition(TRELLIS_DEV_TASK_TEMPLATE_KEY).expect("definition");
        WorkflowDefinitionRepository::create(&store, &definition)
            .await
            .expect("store definition");

        let service = WorkflowRunService::new(&store, &store);
        let run = service
            .start_run(StartWorkflowRunCommand {
                workflow_id: Some(definition.id),
                workflow_key: None,
                target_kind: WorkflowTargetKind::Task,
                target_id: Uuid::new_v4(),
            })
            .await
            .expect("start run");
        let run = service
            .complete_phase(CompleteWorkflowPhaseCommand {
                run_id: run.id,
                phase_key: "start".to_string(),
                summary: Some("done".to_string()),
                record_artifacts: vec![],
            })
            .await
            .expect("complete start");
        let run = service
            .activate_phase(ActivateWorkflowPhaseCommand {
                run_id: run.id,
                phase_key: "implement".to_string(),
                session_binding_id: Some(Uuid::new_v4()),
            })
            .await
            .expect("activate implement");
        let run = service
            .complete_phase(CompleteWorkflowPhaseCommand {
                run_id: run.id,
                phase_key: "implement".to_string(),
                summary: Some("implemented".to_string()),
                record_artifacts: vec![WorkflowRecordArtifactDraft {
                    artifact_type: WorkflowRecordArtifactType::PhaseNote,
                    title: "实现说明".to_string(),
                    content: "已完成 workflow runtime 初版".to_string(),
                }],
            })
            .await
            .expect("complete implement");

        assert_eq!(run.current_phase_key.as_deref(), Some("check"));
        assert_eq!(run.record_artifacts.len(), 1);
        assert_eq!(run.record_artifacts[0].title, "实现说明");
    }
}
