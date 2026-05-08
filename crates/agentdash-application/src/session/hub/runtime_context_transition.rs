//! Workflow runtime context transition 的统一应用入口。
//!
//! 这里刻意放在 Hub 层：transition 应用需要同时触碰 live connector、SessionRuntime、
//! persistence event、Hook runtime 与 Bundle sink。调用方只描述“目标上下文是什么”，
//! 不再各自手写事件 JSON 或 hook 触发顺序。

use std::collections::BTreeSet;

use agentdash_spi::hooks::{CapabilityDelta, SharedHookSessionRuntime};
use serde_json::Value;
use uuid::Uuid;

use super::SessionHub;
use crate::capability::build_capability_delta_markdown;
use crate::session::{
    CapabilitySurface, PendingCapabilitySurfaceTransition, RuntimeContextTransition,
    compute_capability_surface_delta,
};

#[derive(Debug, Clone)]
pub(crate) struct LiveRuntimeContextTransitionInput {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub phase_node: String,
    pub run_id: Option<Uuid>,
    pub lifecycle_key: Option<String>,
    pub workflow_key: Option<String>,
    pub before_surface: Option<CapabilitySurface>,
    pub after_surface: CapabilitySurface,
    pub capability_keys: BTreeSet<String>,
    pub key_delta: CapabilityDelta,
    pub apply_mode: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeContextTransitionOutcome {
    pub capability_delta: Option<CapabilityDelta>,
    pub emitted_capability_change: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingRuntimeContextTransitionInput {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub transition_id: String,
    pub phase_node: String,
    pub run_id: Uuid,
    pub lifecycle_key: String,
    pub before_surface: Option<CapabilitySurface>,
    pub after_surface: CapabilitySurface,
    pub capability_keys: BTreeSet<String>,
    pub source_turn_id: Option<String>,
    pub created_at: i64,
}

impl SessionHub {
    pub(crate) async fn apply_live_runtime_context_transition(
        &self,
        hook_session: &SharedHookSessionRuntime,
        input: LiveRuntimeContextTransitionInput,
    ) -> Result<RuntimeContextTransitionOutcome, String> {
        let surface_changed = input.before_surface.as_ref() != Some(&input.after_surface);
        if input.key_delta.is_empty() && !surface_changed {
            self.emit_runtime_context_changed_hook(&input).await;
            return Ok(RuntimeContextTransitionOutcome {
                capability_delta: None,
                emitted_capability_change: false,
            });
        }

        self.replace_current_capability_surface(&input.session_id, input.after_surface.clone())
            .await
            .map_err(|error| format!("Phase node 能力表面热更新失败: {error}"))?;

        let delta = hook_session.update_capabilities(input.capability_keys.clone());
        let notification_delta = delta.clone().unwrap_or_else(|| input.key_delta.clone());
        let steering_delivery = self
            .push_capability_delta_notification(&input, &notification_delta)
            .await;

        let event = RuntimeContextTransition {
            phase_node: &input.phase_node,
            run_id: input.run_id,
            lifecycle_key: input.lifecycle_key.as_deref(),
            apply_mode: input.apply_mode,
            before_surface: input.before_surface.as_ref(),
            after_surface: &input.after_surface,
            capability_keys: &input.capability_keys,
            steering_delivery,
            surface_changed_override: None,
            steering_capability_delta: Some(&notification_delta),
        }
        .event_payload();

        self.emit_capability_surface_changed(
            &input.session_id,
            input.turn_id.as_deref(),
            event.clone(),
        )
        .await
        .map_err(|error| format!("Phase node capability surface 事件持久化失败: {error}"))?;

        self.emit_capability_changed_hook(&input.session_id, input.turn_id.as_deref(), event)
            .await;

        Ok(RuntimeContextTransitionOutcome {
            capability_delta: delta,
            emitted_capability_change: true,
        })
    }

    pub(crate) async fn enqueue_pending_runtime_context_transition(
        &self,
        input: PendingRuntimeContextTransitionInput,
    ) -> Result<(), String> {
        let surface_changed = input.before_surface.as_ref() != Some(&input.after_surface);
        let transition = RuntimeContextTransition {
            phase_node: &input.phase_node,
            run_id: Some(input.run_id),
            lifecycle_key: Some(&input.lifecycle_key),
            apply_mode: "pending_next_turn",
            before_surface: input.before_surface.as_ref(),
            after_surface: &input.after_surface,
            capability_keys: &input.capability_keys,
            steering_delivery: serde_json::json!({
                "status": "deferred_until_next_turn"
            }),
            surface_changed_override: Some(surface_changed),
            steering_capability_delta: None,
        };
        let Some(pending_transition) = transition.to_pending_capability_surface_transition(
            input.transition_id,
            input.source_turn_id,
            input.created_at,
        ) else {
            return Err(format!(
                "PhaseNode `{}` pending transition 缺少 run/lifecycle 元数据",
                input.phase_node
            ));
        };

        self.enqueue_pending_capability_surface_transition(&input.session_id, pending_transition)
            .await
            .map_err(|error| {
                format!(
                    "PhaseNode `{}` 能力表面 pending transition 写入失败: {error}",
                    input.phase_node
                )
            })?;

        self.emit_capability_surface_changed(
            &input.session_id,
            input.turn_id.as_deref(),
            transition.event_payload(),
        )
        .await
        .map_err(|error| {
            format!(
                "PhaseNode `{}` pending 事件持久化失败: {error}",
                input.phase_node
            )
        })?;

        Ok(())
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        hook_session: Option<&SharedHookSessionRuntime>,
        before_surface: CapabilitySurface,
        transitions: &[PendingCapabilitySurfaceTransition],
    ) {
        if transitions.is_empty() {
            return;
        }

        if let Some(hook_session) = hook_session
            && let Some(last_transition) = transitions.last()
        {
            let _ = hook_session.update_capabilities(last_transition.capability_keys.clone());
        }

        let mut pending_event_before_surface = before_surface;
        for pending in transitions {
            let payload = RuntimeContextTransition {
                phase_node: &pending.phase_node,
                run_id: Some(pending.run_id),
                lifecycle_key: Some(&pending.lifecycle_key),
                apply_mode: "applied_on_next_turn",
                before_surface: Some(&pending_event_before_surface),
                after_surface: &pending.surface,
                capability_keys: &pending.capability_keys,
                steering_delivery: serde_json::json!({ "status": "applied_before_prompt" }),
                surface_changed_override: None,
                steering_capability_delta: None,
            }
            .event_payload();
            pending_event_before_surface = pending.surface.clone();
            let _ = self
                .emit_capability_surface_changed(session_id, Some(turn_id), payload.clone())
                .await;
            self.emit_capability_changed_hook(session_id, Some(turn_id), payload)
                .await;
        }
    }

    async fn push_capability_delta_notification(
        &self,
        input: &LiveRuntimeContextTransitionInput,
        notification_delta: &CapabilityDelta,
    ) -> Value {
        let delta_md = build_capability_delta_markdown(
            &input.phase_node,
            notification_delta,
            &input.capability_keys,
            Some(&compute_capability_surface_delta(
                input.before_surface.as_ref(),
                &input.after_surface,
                &input.capability_keys,
            )),
        );
        match self
            .push_session_notification(&input.session_id, delta_md)
            .await
        {
            Ok(()) => serde_json::json!({
                "status": "accepted_by_connector",
            }),
            Err(error) => {
                tracing::warn!(
                    session_id = %input.session_id,
                    phase_node = %input.phase_node,
                    error = %error,
                    "Phase node capability steering notification delivery failed"
                );
                serde_json::json!({
                    "status": "failed",
                    "error": error.to_string(),
                })
            }
        }
    }

    async fn emit_runtime_context_changed_hook(&self, input: &LiveRuntimeContextTransitionInput) {
        self.emit_capability_changed_hook(
            &input.session_id,
            input.turn_id.as_deref(),
            serde_json::json!({
                "phase_node": input.phase_node.as_str(),
                "run_id": input.run_id.map(|id| id.to_string()),
                "lifecycle_key": input.lifecycle_key.as_deref(),
                "workflow_key": input.workflow_key.as_deref(),
                "apply_mode": input.apply_mode,
                "capability_surface_changed": false,
                "reason": "phase_node_context_changed"
            }),
        )
        .await;
    }
}
