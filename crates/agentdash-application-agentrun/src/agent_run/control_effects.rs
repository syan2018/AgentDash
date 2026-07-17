use std::sync::Arc;

use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectKind, AgentRunControlEffectPort, AgentRunControlEffectStatus,
    AgentRunControlEffectStore, AgentRunDeliveryTerminalConvergencePort,
    AgentRunLifecycleTerminalConvergencePort, AgentRunTerminalControlInput,
    AgentRunTerminalHookEffectPort, AgentRunWaitProducerTerminalConvergencePort,
    NewAgentRunControlEffectRecord,
};
use async_trait::async_trait;

#[derive(Clone)]
pub struct AgentRunControlEffectService {
    deps: Arc<AgentRunControlEffectDeps>,
}

#[derive(Clone)]
pub struct AgentRunControlEffectDeps {
    pub store: Arc<dyn AgentRunControlEffectStore>,
    pub delivery: Arc<dyn AgentRunDeliveryTerminalConvergencePort>,
    pub wait_producer: Arc<dyn AgentRunWaitProducerTerminalConvergencePort>,
    pub lifecycle: Arc<dyn AgentRunLifecycleTerminalConvergencePort>,
    pub terminal_hooks: Arc<dyn AgentRunTerminalHookEffectPort>,
}

impl AgentRunControlEffectService {
    pub fn new(deps: AgentRunControlEffectDeps) -> Self {
        Self {
            deps: Arc::new(deps),
        }
    }

    async fn execute_owner<F, Fut>(
        &self,
        input: &AgentRunTerminalControlInput,
        kind: AgentRunControlEffectKind,
        execute: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let dedup_key = format!(
            "runtime_terminal:{}:{}:{}",
            input.effect_id,
            input.terminal_event_sequence.0,
            kind.as_str()
        );
        let record = self
            .deps
            .store
            .insert_or_get(NewAgentRunControlEffectRecord {
                dedup_key: dedup_key.clone(),
                presentation_thread_id: input.presentation_thread_id.clone(),
                presentation_turn_id: input.presentation_turn_id.clone(),
                terminal_event_sequence: input.terminal_event_sequence,
                effect_kind: kind,
                payload: serde_json::to_value(input).map_err(|error| error.to_string())?,
            })
            .await?;
        if record.status == AgentRunControlEffectStatus::Succeeded {
            return Ok(());
        }
        let Some(claimed) = self
            .deps
            .store
            .claim(&dedup_key, kind.as_str(), 30_000)
            .await?
        else {
            return Err(format!(
                "control effect {dedup_key} is claimed by another worker"
            ));
        };
        let claim_token = claimed.claim_token.ok_or_else(|| {
            format!("claimed control effect {dedup_key} is missing its claim token")
        })?;
        match execute().await {
            Ok(()) => {
                self.deps
                    .store
                    .mark_succeeded(claimed.id, claim_token)
                    .await
            }
            Err(error) => {
                self.deps
                    .store
                    .mark_failed(claimed.id, claim_token, error.clone())
                    .await?;
                Err(error)
            }
        }
    }
}

#[async_trait]
impl AgentRunControlEffectPort for AgentRunControlEffectService {
    async fn observe_runtime_terminal(
        &self,
        input: AgentRunTerminalControlInput,
    ) -> Result<(), String> {
        // Main 的 terminal boundary 先释放/收敛 delivery，再执行产品终态副作用。
        // 各 owner 都必须获得同一 terminal evidence；任一失败会阻止 durable outbox ack，
        // 但不会阻止其他独立 owner 在本轮先完成其幂等收敛。
        let mut errors = Vec::new();
        if let Err(error) = self
            .execute_owner(
                &input,
                AgentRunControlEffectKind::DeliveryConvergence,
                || self.deps.delivery.converge_delivery_terminal(&input),
            )
            .await
        {
            errors.push(error);
        }
        if let Err(error) = self
            .execute_owner(
                &input,
                AgentRunControlEffectKind::WaitProducerTerminalConvergence,
                || {
                    self.deps
                        .wait_producer
                        .converge_wait_producer_terminal(&input)
                },
            )
            .await
        {
            errors.push(error);
        }
        if let Err(error) = self
            .execute_owner(
                &input,
                AgentRunControlEffectKind::LifecycleTerminalConvergence,
                || {
                    self.deps
                        .lifecycle
                        .observe_lifecycle_terminal(&input.presentation_thread_id, input.terminal)
                },
            )
            .await
        {
            errors.push(error);
        }
        if let Err(error) = self
            .execute_owner(
                &input,
                AgentRunControlEffectKind::TerminalHookEffects,
                || self.deps.terminal_hooks.execute_terminal_hooks(&input),
            )
            .await
        {
            errors.push(error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use agentdash_agent_runtime_contract::RuntimeTurnTerminal;
    use agentdash_application_ports::agent_run_control_effect::*;
    use agentdash_test_support::control_effect::MemoryControlEffectStore;
    use async_trait::async_trait;

    use super::*;

    #[derive(Clone)]
    struct RecordingPort {
        name: &'static str,
        calls: Arc<Mutex<Vec<&'static str>>>,
        failures_remaining: Arc<Mutex<usize>>,
    }

    impl RecordingPort {
        fn record(&self) -> Result<(), String> {
            self.calls.lock().expect("calls").push(self.name);
            let mut failures = self.failures_remaining.lock().expect("failures");
            if *failures > 0 {
                *failures -= 1;
                Err(format!("{} failed", self.name))
            } else {
                Ok(())
            }
        }
    }

    #[async_trait]
    impl AgentRunDeliveryTerminalConvergencePort for RecordingPort {
        async fn converge_delivery_terminal(
            &self,
            _: &AgentRunTerminalControlInput,
        ) -> Result<(), String> {
            self.record()
        }
    }

    #[async_trait]
    impl AgentRunWaitProducerTerminalConvergencePort for RecordingPort {
        async fn converge_wait_producer_terminal(
            &self,
            _: &AgentRunTerminalControlInput,
        ) -> Result<(), String> {
            self.record()
        }
    }

    #[async_trait]
    impl AgentRunLifecycleTerminalConvergencePort for RecordingPort {
        async fn observe_lifecycle_terminal(
            &self,
            _: &agentdash_agent_runtime_contract::PresentationThreadId,
            _: RuntimeTurnTerminal,
        ) -> Result<(), String> {
            self.record()
        }
    }

    #[async_trait]
    impl AgentRunTerminalHookEffectPort for RecordingPort {
        async fn execute_terminal_hooks(
            &self,
            _: &AgentRunTerminalControlInput,
        ) -> Result<(), String> {
            self.record()
        }
    }

    fn input() -> AgentRunTerminalControlInput {
        AgentRunTerminalControlInput {
            effect_id: "effect".into(),
            runtime_thread_id: "runtime-thread".parse().expect("runtime thread"),
            presentation_thread_id: "presentation-thread".parse().expect("presentation thread"),
            runtime_turn_id: "runtime-turn".parse().expect("runtime turn"),
            presentation_turn_id: "presentation-turn".parse().expect("presentation turn"),
            terminal_event_sequence: agentdash_agent_runtime_contract::EventSequence(9),
            terminal: RuntimeTurnTerminal::Completed,
            message: None,
            diagnostic: None,
            started_at_ms: Some(10),
            completed_at_ms: 20,
            binding_id: "binding".parse().expect("binding"),
            driver_generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration(3),
            surface_revision: agentdash_agent_runtime_contract::SurfaceRevision(4),
            surface_digest: "surface-digest".parse().expect("surface digest"),
            source_thread_id: "source-thread".into(),
            source_turn_id: Some("source-turn".into()),
            terminal_hook_effect_binding: None,
        }
    }

    fn service(
        calls: Arc<Mutex<Vec<&'static str>>>,
        failing: Option<&'static str>,
    ) -> AgentRunControlEffectService {
        let port = |name| {
            Arc::new(RecordingPort {
                name,
                calls: calls.clone(),
                failures_remaining: Arc::new(Mutex::new(usize::from(failing == Some(name)))),
            })
        };
        AgentRunControlEffectService::new(AgentRunControlEffectDeps {
            store: Arc::new(MemoryControlEffectStore::default()),
            delivery: port("delivery"),
            wait_producer: port("wait"),
            lifecycle: port("lifecycle"),
            terminal_hooks: port("hooks"),
        })
    }

    #[tokio::test]
    async fn terminal_effects_follow_main_boundary_order() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        service(calls.clone(), None)
            .observe_runtime_terminal(input())
            .await
            .expect("terminal effects");
        assert_eq!(
            *calls.lock().expect("calls"),
            ["delivery", "wait", "lifecycle", "hooks"]
        );
    }

    #[tokio::test]
    async fn failure_keeps_independent_terminal_owners_converging_before_retry() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let service = service(calls.clone(), Some("lifecycle"));
        let error = service
            .observe_runtime_terminal(input())
            .await
            .expect_err("lifecycle failure");
        assert_eq!(error, "lifecycle failed");
        assert_eq!(
            *calls.lock().expect("calls"),
            ["delivery", "wait", "lifecycle", "hooks"]
        );
        service
            .observe_runtime_terminal(input())
            .await
            .expect("durable owner retry");
        assert_eq!(
            *calls.lock().expect("calls"),
            ["delivery", "wait", "lifecycle", "hooks", "lifecycle"]
        );
    }

    #[tokio::test]
    async fn same_dedup_key_rejects_different_immutable_evidence() {
        let store = MemoryControlEffectStore::default();
        let original = NewAgentRunControlEffectRecord {
            dedup_key: "runtime_terminal:effect:9:owner".into(),
            presentation_thread_id: "presentation-thread".parse().unwrap(),
            presentation_turn_id: "presentation-turn".parse().unwrap(),
            terminal_event_sequence: agentdash_agent_runtime_contract::EventSequence(9),
            effect_kind: AgentRunControlEffectKind::DeliveryConvergence,
            payload: serde_json::json!({"terminal": "completed"}),
        };
        store.insert_or_get(original.clone()).await.unwrap();
        let mut conflicting = original;
        conflicting.payload = serde_json::json!({"terminal": "failed"});
        let error = store
            .insert_or_get(conflicting)
            .await
            .expect_err("immutable evidence conflict");
        assert_eq!(error, "control effect immutable evidence conflict");
    }
}
