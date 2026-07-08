//! 通用启动对账管线
//!
//! 服务重启后按固定顺序执行：Session 恢复 → Gate wait policy 收束 → Task view 投影 → Infrastructure。
//! Phase 之间存在依赖：Task view 投影依赖 Session 先完成（否则会误判 session 仍在运行）。
//!
//! **定位说明**：本管线只覆盖 projection 方向（session/lifecycle 真相源 → Task view）。
//! 运行期反向（业务终态 → session cancel）的 command 通道见
//! [`crate::reconcile::terminal_cancel`]。

use agentdash_application_workflow::gate::GateProducerTerminalEvent;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::sync::Arc;

use crate::ApplicationError;
use crate::gate_wait_policy::GateProducerTerminalConvergencePort;
use crate::session::SessionRuntimeService;
use crate::task::view_projector::project_task_views_on_boot;
use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectReplayPhase, AgentRunControlEffectReplayPort,
};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{
    AgentRunDeliveryBindingRepository, DeliveryBindingStatus, GateWaitPolicyEnvelope,
    LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchorRepository,
    WaitProducerRef,
};

const GATE_WAIT_POLICY_RECONCILE_LIMIT: usize = 500;
const CONTROL_EFFECT_REPLAY_BATCH_LIMIT: u32 = 100;
const CONTROL_EFFECT_REPLAY_MAX_BATCHES: usize = 20;

/// 启动对账管线的依赖集合
///
/// M2-c：Task view 改为"从 LifecycleRun/step state 反投影"（Scheme A）。
/// projector 通过 `LifecycleSubjectAssociation(kind=Task)` 定位 Task。
pub struct BootReconcileDeps {
    pub session_runtime: SessionRuntimeService,
    pub agent_run_control_effect_replay: Arc<dyn AgentRunControlEffectReplayPort>,
    pub project_repo: Arc<dyn ProjectRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub lifecycle_subject_association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    pub agent_run_delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    pub gate_producer_terminal_convergence: Arc<dyn GateProducerTerminalConvergencePort>,
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
/// 2. **AgentRun delivery 收敛** — 只 replay delivery convergence control effects
/// 3. **Gate wait policy 收束** — 用已 terminal 的 producer 修复 open gate wait policy
/// 4. **Task view 投影** — 根据 LifecycleRun/step state 反投影 Task view
/// 5. **Infrastructure 恢复** — 预留（定时触发器重建等）
pub async fn run_boot_reconcile(deps: &BootReconcileDeps) -> BootReconcileReport {
    let mut phases = Vec::with_capacity(5);

    // ── Phase 1: Session Reconcile ──────────────────────────
    let session_report = run_session_reconcile(&deps.session_runtime).await;
    phases.push(session_report);

    // ── Phase 2: AgentRun Delivery Convergence ──────────────
    let delivery_report =
        run_control_effect_delivery_convergence(deps.agent_run_control_effect_replay.as_ref())
            .await;
    phases.push(delivery_report);

    // ── Phase 3: Gate Wait Policy Terminal Fallback ─────────
    let gate_wait_policy_report = run_gate_wait_policy_reconcile(deps).await;
    phases.push(gate_wait_policy_report);

    // ── Phase 4: Task View Projection ───────────────────────
    let task_report = run_task_view_projection(deps).await;
    phases.push(task_report);

    // ── Phase 5: Infrastructure Restore ─────────────────────
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

async fn run_control_effect_delivery_convergence(
    replay: &dyn AgentRunControlEffectReplayPort,
) -> PhaseReport {
    let phase = "agent_run_delivery_convergence";
    let mut reconciled = 0usize;
    let mut errors = Vec::new();

    for _ in 0..CONTROL_EFFECT_REPLAY_MAX_BATCHES {
        match replay
            .replay_control_effect_outbox_phase(
                AgentRunControlEffectReplayPhase::DeliveryConvergence,
                CONTROL_EFFECT_REPLAY_BATCH_LIMIT,
            )
            .await
        {
            Ok(0) => break,
            Ok(count) => {
                reconciled = reconciled.saturating_add(count);
                if count < CONTROL_EFFECT_REPLAY_BATCH_LIMIT as usize {
                    break;
                }
            }
            Err(error) => {
                let context =
                    DiagnosticErrorContext::new("reconcile.boot", "agent_run_delivery_convergence")
                        .with_field("phase", phase)
                        .with_field("fatal", false);
                diag_error!(
                    Warn,
                    Subsystem::Reconcile,
                    context = &context,
                    error = &std::io::Error::other(error.clone()),
                    phase = phase,
                    fatal = false,
                    "Phase 2 (AgentRun Delivery Convergence) 出错（非致命）"
                );
                errors.push(error);
                break;
            }
        }
    }

    diag!(
        Info,
        Subsystem::Reconcile,
        phase = phase,
        reconciled = reconciled,
        error_count = errors.len(),
        "Phase 2 (AgentRun Delivery Convergence) 完成"
    );

    PhaseReport {
        phase,
        reconciled,
        skipped: 0,
        errors,
    }
}

async fn run_gate_wait_policy_reconcile(deps: &BootReconcileDeps) -> PhaseReport {
    run_gate_wait_policy_reconcile_phase(
        &deps.lifecycle_gate_repo,
        &deps.agent_run_delivery_binding_repo,
        &deps.gate_producer_terminal_convergence,
    )
    .await
}

async fn run_gate_wait_policy_reconcile_phase(
    gate_repo: &Arc<dyn LifecycleGateRepository>,
    delivery_binding_repo: &Arc<dyn AgentRunDeliveryBindingRepository>,
    convergence: &Arc<dyn GateProducerTerminalConvergencePort>,
) -> PhaseReport {
    let phase = "gate_wait_policy_terminal_fallback";
    let gates = match gate_repo
        .list_open_gate_wait_policies(GATE_WAIT_POLICY_RECONCILE_LIMIT)
        .await
    {
        Ok(gates) => gates,
        Err(error) => {
            let context = DiagnosticErrorContext::new("reconcile.boot", "gate_wait_policy_scan")
                .with_field("phase", phase)
                .with_field("fatal", false);
            diag_error!(
                Warn,
                Subsystem::Reconcile,
                context = &context,
                error = &error,
                phase = phase,
                fatal = false,
                "Phase 2 (Gate Wait Policy Terminal Fallback) 扫描失败"
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
            .and_then(GateWaitPolicyEnvelope::from_payload_opt)
        else {
            skipped += 1;
            diag!(
                Debug,
                Subsystem::Reconcile,
                operation = "reconcile.boot.gate_wait_policy",
                stage = "invalid_gate_wait_policy",
                gate_id = %gate.id,
                "boot gate wait policy reconcile skipped an unparsable policy"
            );
            continue;
        };

        let event = match producer_terminal_event_for_gate_wait_policy(
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
                    "reconcile.boot.gate_wait_policy",
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
                    "boot gate wait policy producer fact lookup failed"
                );
                errors.push(error.to_string());
                continue;
            }
        };

        match convergence
            .observe_gate_producer_terminal(event.clone())
            .await
        {
            Ok(result) if result.no_matching_gate_wait_policy() => {
                skipped += 1;
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.gate_wait_policy",
                    stage = "no_matching_gate_wait_policy",
                    gate_id = %gate.id,
                    producer = ?event.producer,
                    terminal_state = %event.terminal_state,
                    delivery_trace_ref = ?event.trace_ref,
                    "boot gate producer terminal fallback found no matching gate wait policy"
                );
            }
            Ok(result) => {
                reconciled += result.outcomes.len();
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.gate_wait_policy",
                    stage = "reconciled",
                    gate_id = %gate.id,
                    producer = ?event.producer,
                    terminal_state = %event.terminal_state,
                    delivery_trace_ref = ?event.trace_ref,
                    outcome_count = result.outcomes.len(),
                    "boot gate producer terminal fallback reconciled terminal producer"
                );
            }
            Err(error) => {
                let context = DiagnosticErrorContext::new(
                    "reconcile.boot.gate_wait_policy",
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
                    "boot gate producer terminal fallback failed"
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
        "Phase 2 (Gate Wait Policy Terminal Fallback) 完成"
    );

    PhaseReport {
        phase,
        reconciled,
        skipped,
        errors,
    }
}

async fn producer_terminal_event_for_gate_wait_policy(
    delivery_binding_repo: &Arc<dyn AgentRunDeliveryBindingRepository>,
    gate: &LifecycleGate,
    declaration: &GateWaitPolicyEnvelope,
) -> Result<Option<GateProducerTerminalEvent>, ApplicationError> {
    match &declaration.wait_policy.source {
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
                    operation = "reconcile.boot.gate_wait_policy",
                    stage = "producer_not_terminal",
                    reason = "binding_missing",
                    gate_id = %gate.id,
                    producer_run_id = %run_id,
                    producer_agent_id = %agent_id,
                    producer_frame_id = ?frame_id,
                    "boot gate wait policy producer binding is unavailable"
                );
                return Ok(None);
            };

            if let Some(expected_frame_id) = frame_id {
                if binding.launch_frame_id != *expected_frame_id {
                    diag!(
                        Debug,
                        Subsystem::Reconcile,
                        operation = "reconcile.boot.gate_wait_policy",
                        stage = "producer_not_terminal",
                        reason = "frame_mismatch",
                        gate_id = %gate.id,
                        producer_run_id = %run_id,
                        producer_agent_id = %agent_id,
                        producer_frame_id = ?frame_id,
                        binding_frame_id = %binding.launch_frame_id,
                        delivery_status = %binding.status,
                        "boot gate wait policy producer binding does not match declared frame"
                    );
                    return Ok(None);
                }
            }

            if binding.status != DeliveryBindingStatus::Terminal {
                diag!(
                    Debug,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.gate_wait_policy",
                    stage = "producer_not_terminal",
                    reason = "delivery_status",
                    gate_id = %gate.id,
                    producer_run_id = %run_id,
                    producer_agent_id = %agent_id,
                    producer_frame_id = ?frame_id,
                    delivery_status = %binding.status,
                    "boot gate wait policy producer is not terminal"
                );
                return Ok(None);
            }

            let Some(terminal_state) = binding.terminal_state.clone() else {
                diag!(
                    Warn,
                    Subsystem::Reconcile,
                    operation = "reconcile.boot.gate_wait_policy",
                    stage = "producer_terminal_fact_incomplete",
                    gate_id = %gate.id,
                    producer_run_id = %run_id,
                    producer_agent_id = %agent_id,
                    delivery_trace_ref = %binding.runtime_session_id,
                    "boot gate wait policy terminal producer is missing terminal_state"
                );
                return Ok(None);
            };

            Ok(Some(GateProducerTerminalEvent {
                producer: declaration.wait_policy.source.clone(),
                terminal_state,
                terminal_message: binding.terminal_message.clone(),
                terminal_diagnostic: binding
                    .terminal_diagnostic
                    .clone()
                    .and_then(|value| serde_json::from_value(value).ok()),
                producer_last_message: None,
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
    use std::sync::{Arc, Mutex};

    use agentdash_application_workflow::gate::{
        GateDeliveryIntent, GateProducerTerminalConvergenceOutcome,
        GateProducerTerminalConvergenceOutcomeKind, GateProducerTerminalConvergenceResult,
    };
    use agentdash_domain::workflow::{
        AgentRunDeliveryBinding, GateWaitPolicy, GateWaitPolicyEnvelope, LifecycleGate,
        RuntimeSessionExecutionAnchor, WaitExpectedResult, WaitProducerRef, WaitTerminalOutcome,
        WaitTerminalPolicy, WaitWakeTarget,
    };
    use agentdash_test_support::workflow::{
        MemoryAgentRunDeliveryBindingRepository, MemoryLifecycleGateRepository,
    };
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[derive(Clone, Copy)]
    enum FakeConvergenceMode {
        Outcome { gate_id: Uuid },
        NoMatch,
        Error,
    }

    struct FakeConvergence {
        mode: FakeConvergenceMode,
        events: Mutex<Vec<GateProducerTerminalEvent>>,
    }

    #[async_trait::async_trait]
    impl GateProducerTerminalConvergencePort for FakeConvergence {
        async fn observe_gate_producer_terminal(
            &self,
            event: GateProducerTerminalEvent,
        ) -> Result<GateProducerTerminalConvergenceResult, ApplicationError> {
            self.events.lock().unwrap().push(event);
            match self.mode {
                FakeConvergenceMode::Outcome { gate_id } => {
                    Ok(GateProducerTerminalConvergenceResult {
                        outcomes: vec![GateProducerTerminalConvergenceOutcome {
                            gate_id,
                            kind: GateProducerTerminalConvergenceOutcomeKind::Resolved,
                            result_status: Some("failed".to_string()),
                            delivery_intents: Vec::<GateDeliveryIntent>::new(),
                        }],
                    })
                }
                FakeConvergenceMode::NoMatch => Ok(GateProducerTerminalConvergenceResult {
                    outcomes: Vec::new(),
                }),
                FakeConvergenceMode::Error => Err(ApplicationError::Conflict(
                    "parent delivery binding unavailable".to_string(),
                )),
            }
        }
    }

    fn open_gate_wait_policy_gate(
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
        let declaration = GateWaitPolicyEnvelope::new(GateWaitPolicy {
            source: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id: child_agent_id,
                frame_id: Some(child_frame_id),
            },
            expected_result: WaitExpectedResult {
                kind: "companion_result".to_string(),
                correlation_ref: Some("dispatch-1".to_string()),
            },
            terminal_policy: WaitTerminalPolicy {
                failed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "runtime_terminal_failed".to_string(),
                },
                interrupted: WaitTerminalOutcome {
                    status: "cancelled".to_string(),
                    failure_kind: "runtime_terminal_cancelled".to_string(),
                },
                completed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "missing_companion_respond".to_string(),
                },
            },
            wake_target: WaitWakeTarget {
                namespace: "companion".to_string(),
                target_run_id: run_id,
                target_agent_id: parent_agent_id,
                client_command_id: format!("companion-result:{}", gate.id),
            },
        })
        .with_display_value("companion_label", json!("reviewer"));
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
                None,
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
        Arc<MemoryLifecycleGateRepository>,
        Arc<MemoryAgentRunDeliveryBindingRepository>,
        Arc<FakeConvergence>,
    ) {
        let gate_repo = Arc::new(MemoryLifecycleGateRepository::default());
        let delivery_repo = Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
        let convergence = Arc::new(FakeConvergence {
            mode,
            events: Mutex::new(Vec::new()),
        });
        let gate = open_gate_wait_policy_gate(
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
        gate_repo: Arc<MemoryLifecycleGateRepository>,
        delivery_repo: Arc<MemoryAgentRunDeliveryBindingRepository>,
        convergence: Arc<FakeConvergence>,
    ) -> PhaseReport {
        let gate_repo: Arc<dyn LifecycleGateRepository> = gate_repo;
        let delivery_repo: Arc<dyn AgentRunDeliveryBindingRepository> = delivery_repo;
        let convergence: Arc<dyn GateProducerTerminalConvergencePort> = convergence;
        run_gate_wait_policy_reconcile_phase(&gate_repo, &delivery_repo, &convergence).await
    }

    #[tokio::test]
    async fn gate_wait_policy_reconcile_observes_terminal_agent_run_delivery() {
        let run_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let gate =
            open_gate_wait_policy_gate(run_id, child_agent_id, child_frame_id, Uuid::new_v4());
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
    async fn gate_wait_policy_reconcile_skips_non_terminal_producer() {
        let gate = open_gate_wait_policy_gate(
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
    async fn gate_wait_policy_reconcile_reports_no_matching_policy_as_skipped() {
        let gate = open_gate_wait_policy_gate(
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
    async fn gate_wait_policy_reconcile_reports_fallback_failure_as_error() {
        let gate = open_gate_wait_policy_gate(
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
