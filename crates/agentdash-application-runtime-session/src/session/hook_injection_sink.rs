use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use agentdash_spi::hooks::{HookInjection, HookTrigger};

use super::runtime_registry::SessionRuntimeRegistry;
use crate::context::{AuditTrigger, SharedContextAuditBus, emit_fragment};
use crate::hooks::hook_injection_to_fragment;

pub type DynRuntimeHookInjectionSink = Arc<dyn RuntimeHookInjectionSink>;

#[derive(Debug, Clone)]
pub enum RuntimeInjectionSource {
    Hook(HookTrigger),
    RuntimeContextUpdate,
}

impl RuntimeInjectionSource {
    fn audit_label(&self) -> String {
        match self {
            Self::Hook(trigger) => format!("{trigger:?}"),
            Self::RuntimeContextUpdate => "runtime_context_update".to_string(),
        }
    }
}

#[async_trait]
pub trait RuntimeHookInjectionSink: Send + Sync {
    async fn emit_injections(
        &self,
        session_id: &str,
        source: RuntimeInjectionSource,
        injections: &[HookInjection],
    );
}

pub(super) struct SessionRuntimeHookInjectionSink {
    registry: SessionRuntimeRegistry,
    audit_bus: Option<SharedContextAuditBus>,
}

impl SessionRuntimeHookInjectionSink {
    pub(super) fn new(
        registry: SessionRuntimeRegistry,
        audit_bus: Option<SharedContextAuditBus>,
    ) -> Self {
        Self {
            registry,
            audit_bus,
        }
    }
}

#[async_trait]
impl RuntimeHookInjectionSink for SessionRuntimeHookInjectionSink {
    async fn emit_injections(
        &self,
        session_id: &str,
        source: RuntimeInjectionSource,
        injections: &[HookInjection],
    ) {
        if injections.is_empty() {
            return;
        }

        let fragments = injections
            .iter()
            .cloned()
            .map(hook_injection_to_fragment)
            .collect::<Vec<_>>();
        let (bundle_id, bundle_session_uuid) = self
            .registry
            .with_runtime_mut(session_id, |runtime| {
                if let Some(turn) = runtime.and_then(|runtime| runtime.turn_state.active_turn_mut())
                {
                    let bundle_id = turn.context_audit_bundle_id;
                    let bundle_session_uuid = turn.context_audit_session_id;
                    turn.runtime_injection_fragments.extend(fragments.clone());
                    (bundle_id, bundle_session_uuid)
                } else {
                    (Uuid::new_v4(), Uuid::new_v4())
                }
            })
            .await;

        let Some(bus) = self.audit_bus.as_ref() else {
            return;
        };
        let trigger_label = source.audit_label();
        for fragment in fragments {
            emit_fragment(
                bus.as_ref(),
                bundle_id,
                session_id,
                bundle_session_uuid,
                AuditTrigger::HookInjection {
                    trigger: trigger_label.clone(),
                },
                &fragment,
            );
        }
    }
}
