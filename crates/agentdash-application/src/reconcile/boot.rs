//! 通用启动对账管线
//!
//! 服务重启后按固定顺序执行：Session 恢复 → Wait obligation 收束 → Task view 投影 → Infrastructure。
//! Phase 之间存在依赖：Task view 投影依赖 Session 先完成（否则会误判 session 仍在运行）。
//!
//! **定位说明**：本管线只覆盖 projection 方向（session/lifecycle 真相源 → Task view）。
//! 运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。

use agentdash_application_workflow::gate::{
    WaitObligationConvergenceResult, WaitProducerTerminalEvent,
};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use async_trait::async_trait;
use std::sync::Arc;

use crate::ApplicationError;
use crate::companion::CompanionGateControlService;
use crate::session::SessionRuntimeService;
use crate::task::view_projector::project_task_views_on_boot;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{
    AgentRunDeliveryBindingRepository, DeliveryBindingStatus, LifecycleAgentRepository,
    LifecycleGate, LifecycleGateRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchorRepository,
    WaitObligationDeclaration, WaitProducerRef,
};

const WAIT_OBLIGATION_RECONCILE_LIMIT: usize = 500;

#[async_trait]
pub trait WaitObligationTerminalConvergencePort: Send + Sync {
    async fn observe_wait_producer_terminal(
        &self,
        event: WaitProducerTerminalEvent,
    ) -> Result<WaitObligationConvergenceResult, ApplicationError>;
}

#[async_trait]
impl WaitObligationTerminalConvergencePort for CompanionGateControlService {
    async fn observe_wait_producer_terminal(
        &self,
        event: WaitProducerTerminalEvent,
    ) -> Result<WaitObligationConvergenceResult, ApplicationError> {
        CompanionGateControlService::observe_wait_producer_terminal(self, event).await
    }
}

/// 启动对账管线的依赖集合
///
/// M2-c：Task view 改为"从 LifecycleRun/step state 反投影"（Scheme A）。
/// projector 通过 `LifecycleSubjectAssociation(kind=Task)` 定位 Task。
pub struct BootReconcileDeps {
    pub session_runtime: SessionRuntimeService,
    pub project_repo: Arc<dyn ProjectRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    pub agent_run_delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub wait_obligation_terminal_convergence: Arc<dyn WaitObligationTerminalConvergencePort>,
}

/// 单阶段对账结果
#[derive(Debug)]
pub struct PhaseReport {
    pub phase: &'static str,
    pub reconciled: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// 完整管线执行结果
#[derive(Debug)]
pub struct BootReconcileReport {
    pub phases: Vec<PhaseReport>,
}

impl BootReconcileReport {
    pub fn total_reconciled(&self) -> usize {
        self.phases.iter().map(|p| p.reconciled).sum()
    }

    pub fn total_skipped(&self) -> usize {
        self.phases.iter().map(|p| p.skipped).sum()
    }

    pub fn has_errors(&self) -> bool {
        self.phases.iter().any(|p| !p.errors.is_empty())
    }
}

/// 执行完整的启动对账管线。
///
/// 阶段执行顺序固定且不可跳过：
/// 1. **Session 恢复** — 将残留 running 状态的 session 标记为 interrupted
/// 2. **Wait obligation 收束** — 用已 terminal 的 producer 修复 open wait obligation
/// 3. **Task view 投影** — 根据 LifecycleRun/step state 反投影 Task view
/// 4. **Infrastructure 恢复** — 预留（定时触发器重建等）
pub async fn run_boot_reconcile(deps: &BootReconcileDeps) -> BootReconcileReport {
    let mut phases = Vec::with_capacity(4);

    // ── Phase 1: Session Reconcile ──────────────────────────
    let session_report = run_session_reconcile(&deps.session_runtime).await;
    phases.push(session_report);

    // ── Phase 2: Wait Obligation Terminal Convergence ───────
    let wait_obligation_report = run_wait_obligation_reconcile(deps).await;
    phases.push(wait_obligation_report);

    // ── Phase 3: Task View Projection ───────────────────────
    let task_report = run_task_view_projection(deps).await;
    phases.push(task_report);

    // ── Phase 4: Infrastructure Restore ─────────────────────
    // 目前仅占位，后续 tick-loop 触发器重建等逻辑在此扩展
    phases.push(PhaseReport {
        phase: "infrastructure_restore",
        reconciled: 0,
        skipped: 0,
        errors: Vec::new(),
    });

    let report = BootReconcileReport { phases };

    diag!(
        Info,
        Subsystem::Reconcile,
        total_reconciled = report.total_reconciled(),
        total_skipped = report.total_skipped(),
        has_errors = report.has_errors(),
        "启动对账管线执行完成"
    );

    report
}

async fn run_session_reconcile(session_runtime: &SessionRuntimeService) -> PhaseReport {
    match session_runtime.recover_interrupted_sessions().await {
        Ok(()) => {
            diag!(
                Info,
                Subsystem::Reconcile,
                "Phase 1 (Session Recovery) 完成"
            );
            PhaseReport {
                phase: "session_recovery",
                reconciled: 0, // recover_interrupted_sessions 暂未返回计数
                skipped: 0,
                errors: Vec::new(),
            }
        }
        Err(err) => {
            let context = DiagnosticErrorContext::new("reconcile.boot", "session_recovery")
                .with_field("phase", "session_recovery")
                .with_field("fatal", false);
            diag_error!(
                Warn,
                Subsystem::Reconcile,
                context = &context,
                error = &err,
                phase = "session_recovery",
                fatal = false,
                "Phase 1 (Session Recovery) 出错（非致命）"
            );
            PhaseReport {
                phase: "session_recovery",
                reconciled: 0,
                skipped: 0,
                errors: vec![err.to_string()],
            }
        }
    }
}

async fn run_wait_obligation_reconcile(deps: &BootReconcileDeps) -> PhaseReport {
    run_wait_obligation_reconcile_phase(
        &deps.lifecycle_gate_repo,
        &deps.agent_run_delivery_binding_repo,
        &deps.wait_obligation_terminal_convergence,
    )
    .await
}

async fn run_wait_obligation_reconcile_phase(
    gate_repo: &Arc<dyn LifecycleGateRepository>,
    delivery_binding_repo: &Arc<dyn AgentRunDeliveryBindingRepository>,
    convergence: &Arc<dyn WaitObligationTerminalConvergencePort>,
) -> PhaseReport {
    let phase = "wait_obligation_convergence";
    let gates = match gate_repo
        .list_open_wait_obligations(WAIT_OBLIGATION_RECONCILE_LIMIT)
        .await
    {
        Ok(gates) => gates,
        Err(error) => {
            let context = DiagnosticErrorContext::new("reconcile.boot", "wait_obligation_scan")
                .with_field("phase", phase)
                .with_field("fatal", false);
            diag_error!(
                Warn,
                Subsystem::Reconcile,
                context = &context,
                error = &error,
                phase = phase,
                fatal = false,
                "Phase 2 (Wait Obligation Convergence) 扫描失败"
            );
            return PhaseReport {
                phase,
                reconciled: 0,
                skipped: 0,
                errors: vec![error.to_string()],
            };
        }
    };

    let mut reconciled = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();

    for gate in gates {
        let Some(declaration) = gate
            .payload_json
            .as_ref()
            .and_then(WaitObligationDeclaration::from_payload)
        else {
            skipped += 1;
            diag!(
                Debug,
                Subsystem::Reconcile,
                operation = "reconcile.boot.wait_obligation",
                stage = "invalid_wait_obligation_declaration",
                gate_id = %gate.id,
                "boot wait obligation reconcile skipped an unparsable declaration"
            );
            continue;
        };

        let event = match producer_terminal_event_for_obligation(
            delivery_binding_repo,
            &gate,
            &declaration,
        )
        .await
        {
            Ok(Some(event)) => event,
            Ok(None) => {
                skipped += 1;
                continue;
            }
            Err(error) => {
                let context = DiagnosticErrorContext::new(
                    "reconcile.boot.wait_obligation",
                    "producer_fact_lookup",
                )
                .with_field("phase", phase)
                .with_field("gate_id", gate.id.to_string());
                diag_error!(
                    Warn,
                    Subsystem::Reconcile,
                    context = &context,
                    error = &error,
                    phase = phase,
                    gate_id = %gate.id,
                    "boot wait obligation producer fact lookup failed"
                );
                errors.push(error.to_string());
                continue;
            }
        };

        match convergence
            .observe_wait_producer_terminal(event.clone())
            .await
        {
            Ok(result) if result.no_matching_obligation() => {
                skipped += 1;
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.wait_obligation",
                    stage = "no_matching_obligation",
                    gate_id = %gate.id,
                    producer = ?event.producer,
                    terminal_state = %event.terminal_state,
                    delivery_trace_ref = ?event.trace_ref,
                    "boot wait obligation convergence found no matching obligation"
                );
            }
            Ok(result) => {
                reconciled += result.outcomes.len();
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.wait_obligation",
                    stage = "reconciled",
                    gate_id = %gate.id,
                    producer = ?event.producer,
                    terminal_state = %event.terminal_state,
                    delivery_trace_ref = ?event.trace_ref,
                    outcome_count = result.outcomes.len(),
                    "boot wait obligation convergence reconciled terminal producer"
                );
            }
            Err(error) => {
                let context = DiagnosticErrorContext::new(
                    "reconcile.boot.wait_obligation",
                    "convergence_failure",
                )
                .with_field("phase", phase)
                .with_field("gate_id", gate.id.to_string());
                diag_error!(
                    Warn,
                    Subsystem::Reconcile,
                    context = &context,
                    error = &error,
                    phase = phase,
                    gate_id = %gate.id,
                    producer = ?event.producer,
                    terminal_state = %event.terminal_state,
                    delivery_trace_ref = ?event.trace_ref,
                    "boot wait obligation convergence failed"
                );
                errors.push(error.to_string());
            }
        }
    }

    diag!(
        Info,
        Subsystem::Reconcile,
        phase = phase,
        reconciled = reconciled,
        skipped = skipped,
        error_count = errors.len(),
        "Phase 2 (Wait Obligation Convergence) 完成"
    );

    PhaseReport {
        phase,
        reconciled,
        skipped,
        errors,
    }
}

async fn producer_terminal_event_for_obligation(
    delivery_binding_repo: &Arc<dyn AgentRunDeliveryBindingRepository>,
    gate: &LifecycleGate,
    declaration: &WaitObligationDeclaration,
) -> Result<Option<WaitProducerTerminalEvent>, ApplicationError> {
    match &declaration.wait_source.producer {
        WaitProducerRef::AgentRunDelivery {
            run_id,
            agent_id,
            frame_id,
        } => {
            let Some(binding) = delivery_binding_repo
                .get_current(*run_id, *agent_id)
                .await
                .map_err(ApplicationError::from)?
            else {
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.wait_obligation",
                    stage = "producer_not_terminal",
                    reason = "binding_missing",
                    gate_id = %gate.id,
                    producer_run_id = %run_id,
                    producer_agent_id = %agent_id,
                    producer_frame_id = ?frame_id,
                    "boot wait obligation producer binding is unavailable"
                );
                return Ok(None);
            };

            if let Some(expected_frame_id) = frame_id {
                if binding.launch_frame_id != *expected_frame_id {
                    diag!(
                        Debug,
                        Subsystem::Reconcile,
                        operation = "reconcile.boot.wait_obligation",
                        stage = "producer_not_terminal",
                        reason = "frame_mismatch",
                        gate_id = %gate.id,
                        producer_run_id = %run_id,
                        producer_agent_id = %agent_id,
                        producer_frame_id = ?frame_id,
                        binding_frame_id = %binding.launch_frame_id,
                        delivery_status = %binding.status,
                        "boot wait obligation producer binding does not match declared frame"
                    );
                    return Ok(None);
                }
            }

            if binding.status != DeliveryBindingStatus::Terminal {
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.wait_obligation",
                    stage = "producer_not_terminal",
                    reason = "delivery_status",
                    gate_id = %gate.id,
                    producer_run_id = %run_id,
                    producer_agent_id = %agent_id,
                    producer_frame_id = ?frame_id,
                    delivery_status = %binding.status,
                    "boot wait obligation producer is not terminal"
                );
                return Ok(None);
            }

            let Some(terminal_state) = binding.terminal_state.clone() else {
                diag!(
                    Warn,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.wait_obligation",
                    stage = "producer_terminal_fact_incomplete",
                    gate_id = %gate.id,
                    producer_run_id = %run_id,
                    producer_agent_id = %agent_id,
                    delivery_trace_ref = %binding.runtime_session_id,
                    "boot wait obligation terminal producer is missing terminal_state"
                );
                return Ok(None);
            };

            Ok(Some(WaitProducerTerminalEvent {
                producer: declaration.wait_source.producer.clone(),
                terminal_state,
                terminal_message: binding.terminal_message.clone(),
                source_turn_id: binding.last_turn_id.clone(),
                trace_ref: Some(binding.runtime_session_id.clone()),
            }))
        }
    }
}

async fn run_task_view_projection(deps: &BootReconcileDeps) -> PhaseReport {
    match project_task_views_on_boot(
        &deps.project_repo,
        &deps.state_change_repo,
        &deps.story_repo,
        &deps.lifecycle_subject_association_repo,
        &deps.lifecycle_run_repo,
        &deps.lifecycle_agent_repo,
        &deps.execution_anchor_repo,
    )
    .await
    {
        Ok(()) => {
            diag!(
                Info,
                Subsystem::Reconcile,
                "Phase 3 (Task View Projection) 完成"
            );
            PhaseReport {
                phase: "task_view_projection",
                reconciled: 0,
                skipped: 0,
                errors: Vec::new(),
            }
        }
        Err(err) => {
            let context = DiagnosticErrorContext::new("reconcile.boot", "task_view_projection")
                .with_field("phase", "task_view_projection")
                .with_field("fatal", true);
            diag_error!(
                Error,
                Subsystem::Reconcile,
                context = &context,
                error = &err,
                phase = "task_view_projection",
                fatal = true,
                "Phase 3 (Task View Projection) 失败"
            );
            PhaseReport {
                phase: "task_view_projection",
                reconciled: 0,
                skipped: 0,
                errors: vec![err.to_string()],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use agentdash_application_workflow::gate::{
        GateDeliveryIntent, GateNotificationIntent, WaitObligationConvergenceOutcome,
        WaitObligationConvergenceOutcomeKind,
    };
    use agentdash_domain::{
        DomainError,
        workflow::{
            AgentRunDeliveryBinding, LifecycleGate, RuntimeSessionExecutionAnchor,
            WaitObligationDeclaration,
        },
    };
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct MemoryGateRepo {
        gates: Mutex<Vec<LifecycleGate>>,
    }

    #[async_trait::async_trait]
    impl LifecycleGateRepository for MemoryGateRepo {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.lock().unwrap().push(gate.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .iter()
                .find(|gate| gate.id == id)
                .cloned())
        }

        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .iter()
                .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
                .cloned()
                .collect())
        }

        async fn list_open_wait_obligations(
            &self,
            limit: usize,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .iter()
                .filter(|gate| {
                    gate.is_open()
                        && gate
                            .payload_json
                            .as_ref()
                            .and_then(WaitObligationDeclaration::from_payload)
                            .is_some()
                })
                .take(limit)
                .cloned()
                .collect())
        }

        async fn list_by_wait_producer(
            &self,
            producer: &WaitProducerRef,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .iter()
                .filter(|gate| {
                    gate.payload_json
                        .as_ref()
                        .and_then(WaitObligationDeclaration::from_payload)
                        .is_some_and(|declaration| declaration.wait_source.producer == *producer)
                })
                .cloned()
                .collect())
        }

        async fn find_by_agent_and_correlation(
            &self,
            agent_id: Uuid,
            correlation_id: &str,
        ) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .lock()
                .unwrap()
                .iter()
                .find(|gate| {
                    gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id
                })
                .cloned())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            let mut gates = self.gates.lock().unwrap();
            if let Some(existing) = gates.iter_mut().find(|existing| existing.id == gate.id) {
                *existing = gate.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryDeliveryBindingRepo {
        bindings: Mutex<HashMap<(Uuid, Uuid), AgentRunDeliveryBinding>>,
    }

    #[async_trait::async_trait]
    impl AgentRunDeliveryBindingRepository for MemoryDeliveryBindingRepo {
        async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
            self.bindings
                .lock()
                .unwrap()
                .insert((binding.run_id, binding.agent_id), binding.clone());
            Ok(())
        }

        async fn get_current(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .get(&(run_id, agent_id))
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .values()
                .filter(|binding| binding.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.bindings
                .lock()
                .unwrap()
                .retain(|_, binding| binding.runtime_session_id != runtime_session_id);
            Ok(())
        }
    }

    #[derive(Clone, Copy)]
    enum FakeConvergenceMode {
        Outcome { gate_id: Uuid },
        NoMatch,
        Error,
    }

    struct FakeConvergence {
        mode: FakeConvergenceMode,
        events: Mutex<Vec<WaitProducerTerminalEvent>>,
    }

    #[async_trait::async_trait]
    impl WaitObligationTerminalConvergencePort for FakeConvergence {
        async fn observe_wait_producer_terminal(
            &self,
            event: WaitProducerTerminalEvent,
        ) -> Result<WaitObligationConvergenceResult, ApplicationError> {
            self.events.lock().unwrap().push(event);
            match self.mode {
                FakeConvergenceMode::Outcome { gate_id } => Ok(WaitObligationConvergenceResult {
                    outcomes: vec![WaitObligationConvergenceOutcome {
                        gate_id,
                        kind: WaitObligationConvergenceOutcomeKind::Resolved,
                        result_status: Some("failed".to_string()),
                        delivery_intents: Vec::<GateDeliveryIntent>::new(),
                        notification_intents: Vec::<GateNotificationIntent>::new(),
                    }],
                }),
                FakeConvergenceMode::NoMatch => Ok(WaitObligationConvergenceResult {
                    outcomes: Vec::new(),
                }),
                FakeConvergenceMode::Error => Err(ApplicationError::Conflict(
                    "parent delivery binding unavailable".to_string(),
                )),
            }
        }
    }

    fn open_wait_obligation_gate(
        run_id: Uuid,
        child_agent_id: Uuid,
        child_frame_id: Uuid,
        parent_agent_id: Uuid,
    ) -> LifecycleGate {
        let mut gate = LifecycleGate::open(
            run_id,
            Some(child_agent_id),
            Some(child_frame_id),
            "companion_wait_follow_up",
            "dispatch-1",
            Some(json!({ "companion_label": "reviewer" })),
        );
        let declaration = WaitObligationDeclaration::companion_agent_run_delivery(
            run_id,
            child_agent_id,
            Some(child_frame_id),
            "dispatch-1",
            run_id,
            parent_agent_id,
            gate.id,
        );
        gate.payload_json = Some(
            declaration
                .write_into_payload(gate.payload_json.take())
                .expect("declaration payload should serialize"),
        );
        gate
    }

    fn binding_for_gate(
        gate: &LifecycleGate,
        status: DeliveryBindingStatus,
    ) -> AgentRunDeliveryBinding {
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "child-runtime".to_string(),
            gate.run_id,
            gate.frame_id.expect("gate frame"),
            gate.agent_id.expect("gate agent"),
        );
        let binding = AgentRunDeliveryBinding::from_anchor(&anchor, status, Utc::now());
        if status == DeliveryBindingStatus::Terminal {
            binding.mark_terminal(
                "child-turn",
                "failed",
                Some("provider failed".to_string()),
                Utc::now(),
            )
        } else {
            binding
        }
    }

    async fn seed_phase_inputs(
        binding: AgentRunDeliveryBinding,
        mode: FakeConvergenceMode,
    ) -> (
        Arc<MemoryGateRepo>,
        Arc<MemoryDeliveryBindingRepo>,
        Arc<FakeConvergence>,
    ) {
        let gate_repo = Arc::new(MemoryGateRepo::default());
        let delivery_repo = Arc::new(MemoryDeliveryBindingRepo::default());
        let convergence = Arc::new(FakeConvergence {
            mode,
            events: Mutex::new(Vec::new()),
        });
        let gate = open_wait_obligation_gate(
            binding.run_id,
            binding.agent_id,
            binding.launch_frame_id,
            Uuid::new_v4(),
        );
        gate_repo.create(&gate).await.expect("seed gate");
        delivery_repo.upsert(&binding).await.expect("seed binding");
        (gate_repo, delivery_repo, convergence)
    }

    async fn run_phase_with_fakes(
        gate_repo: Arc<MemoryGateRepo>,
        delivery_repo: Arc<MemoryDeliveryBindingRepo>,
        convergence: Arc<FakeConvergence>,
    ) -> PhaseReport {
        let gate_repo: Arc<dyn LifecycleGateRepository> = gate_repo;
        let delivery_repo: Arc<dyn AgentRunDeliveryBindingRepository> = delivery_repo;
        let convergence: Arc<dyn WaitObligationTerminalConvergencePort> = convergence;
        run_wait_obligation_reconcile_phase(&gate_repo, &delivery_repo, &convergence).await
    }

    #[tokio::test]
    async fn wait_obligation_reconcile_observes_terminal_agent_run_delivery() {
        let run_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate =
            open_wait_obligation_gate(run_id, child_agent_id, child_frame_id, Uuid::new_v4());
        let binding = binding_for_gate(&gate, DeliveryBindingStatus::Terminal);
        let gate_id = gate.id;
        let (gate_repo, delivery_repo, convergence) =
            seed_phase_inputs(binding, FakeConvergenceMode::Outcome { gate_id }).await;

        let report = run_phase_with_fakes(gate_repo, delivery_repo, convergence.clone()).await;

        assert_eq!(report.reconciled, 1);
        assert_eq!(report.skipped, 0);
        assert!(report.errors.is_empty());
        let events = convergence.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].terminal_state, "failed");
        assert_eq!(
            events[0].terminal_message.as_deref(),
            Some("provider failed")
        );
        assert_eq!(events[0].source_turn_id.as_deref(), Some("child-turn"));
        assert_eq!(events[0].trace_ref.as_deref(), Some("child-runtime"));
        assert_eq!(
            events[0].producer,
            WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame_id)
            }
        );
    }

    #[tokio::test]
    async fn wait_obligation_reconcile_skips_non_terminal_producer() {
        let gate = open_wait_obligation_gate(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let binding = binding_for_gate(&gate, DeliveryBindingStatus::Running);
        let gate_id = gate.id;
        let (gate_repo, delivery_repo, convergence) =
            seed_phase_inputs(binding, FakeConvergenceMode::Outcome { gate_id }).await;

        let report = run_phase_with_fakes(gate_repo, delivery_repo, convergence.clone()).await;

        assert_eq!(report.reconciled, 0);
        assert_eq!(report.skipped, 1);
        assert!(report.errors.is_empty());
        assert!(convergence.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn wait_obligation_reconcile_reports_no_matching_obligation_as_skipped() {
        let gate = open_wait_obligation_gate(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let binding = binding_for_gate(&gate, DeliveryBindingStatus::Terminal);
        let (gate_repo, delivery_repo, convergence) =
            seed_phase_inputs(binding, FakeConvergenceMode::NoMatch).await;

        let report = run_phase_with_fakes(gate_repo, delivery_repo, convergence.clone()).await;

        assert_eq!(report.reconciled, 0);
        assert_eq!(report.skipped, 1);
        assert!(report.errors.is_empty());
        assert_eq!(convergence.events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn wait_obligation_reconcile_reports_convergence_failure_as_error() {
        let gate = open_wait_obligation_gate(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let binding = binding_for_gate(&gate, DeliveryBindingStatus::Terminal);
        let (gate_repo, delivery_repo, convergence) =
            seed_phase_inputs(binding, FakeConvergenceMode::Error).await;

        let report = run_phase_with_fakes(gate_repo, delivery_repo, convergence.clone()).await;

        assert_eq!(report.reconciled, 0);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(convergence.events.lock().unwrap().len(), 1);
    }
}
