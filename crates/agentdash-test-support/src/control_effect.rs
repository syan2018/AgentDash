use std::collections::HashMap;

use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectRecord, AgentRunControlEffectStatus, AgentRunControlEffectStore,
    NewAgentRunControlEffectRecord,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

/// In-memory terminal-control-effect store for application-port tests.
#[derive(Default)]
pub struct MemoryControlEffectStore {
    records: Mutex<HashMap<String, AgentRunControlEffectRecord>>,
}

#[async_trait]
impl AgentRunControlEffectStore for MemoryControlEffectStore {
    async fn insert_or_get(
        &self,
        effect: NewAgentRunControlEffectRecord,
    ) -> Result<AgentRunControlEffectRecord, String> {
        let mut records = self.records.lock().await;
        let expected = effect.clone();
        let record = records
            .entry(effect.dedup_key.clone())
            .or_insert_with(|| AgentRunControlEffectRecord {
                id: uuid::Uuid::new_v4(),
                dedup_key: effect.dedup_key,
                presentation_thread_id: effect.presentation_thread_id,
                presentation_turn_id: effect.presentation_turn_id,
                terminal_event_sequence: effect.terminal_event_sequence,
                effect_kind: effect.effect_kind,
                payload: effect.payload,
                status: AgentRunControlEffectStatus::Pending,
                claim_token: None,
            })
            .clone();
        if record.presentation_thread_id != expected.presentation_thread_id
            || record.presentation_turn_id != expected.presentation_turn_id
            || record.terminal_event_sequence != expected.terminal_event_sequence
            || record.effect_kind != expected.effect_kind
            || record.payload != expected.payload
        {
            return Err("control effect immutable evidence conflict".into());
        }
        Ok(record)
    }

    async fn claim(
        &self,
        dedup_key: &str,
        _: &str,
        _: i64,
    ) -> Result<Option<AgentRunControlEffectRecord>, String> {
        let mut records = self.records.lock().await;
        let record = records
            .get_mut(dedup_key)
            .ok_or_else(|| format!("control effect record is missing: {dedup_key}"))?;
        if record.status == AgentRunControlEffectStatus::Succeeded {
            return Ok(None);
        }
        record.status = AgentRunControlEffectStatus::Running;
        record.claim_token = Some(uuid::Uuid::new_v4());
        Ok(Some(record.clone()))
    }

    async fn mark_succeeded(
        &self,
        effect_id: uuid::Uuid,
        claim_token: uuid::Uuid,
    ) -> Result<(), String> {
        let mut records = self.records.lock().await;
        let record = records
            .values_mut()
            .find(|record| record.id == effect_id)
            .ok_or_else(|| format!("control effect record is missing: {effect_id}"))?;
        if record.claim_token != Some(claim_token) {
            return Err("control effect claim token does not match".into());
        }
        record.status = AgentRunControlEffectStatus::Succeeded;
        record.claim_token = None;
        Ok(())
    }

    async fn mark_failed(
        &self,
        effect_id: uuid::Uuid,
        claim_token: uuid::Uuid,
        _: String,
    ) -> Result<(), String> {
        let mut records = self.records.lock().await;
        let record = records
            .values_mut()
            .find(|record| record.id == effect_id)
            .ok_or_else(|| format!("control effect record is missing: {effect_id}"))?;
        if record.claim_token != Some(claim_token) {
            return Err("control effect claim token does not match".into());
        }
        record.status = AgentRunControlEffectStatus::Failed;
        record.claim_token = None;
        Ok(())
    }
}
