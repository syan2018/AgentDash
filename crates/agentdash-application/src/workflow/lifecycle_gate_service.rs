//! LifecycleGateService — durable wait/resume 机制。
//!
//! companion_request(wait=true) 创建 gate 并 poll 等待 resolve；
//! companion_respond 通过 gate_id resolve gate。

use uuid::Uuid;

use agentdash_domain::workflow::LifecycleGate;

use crate::repository_set::RepositorySet;

pub struct LifecycleGateService {
    repos: RepositorySet,
}

impl LifecycleGateService {
    pub fn new(repos: RepositorySet) -> Self {
        Self { repos }
    }

    /// 创建一个新的 gate 并返回。调用方应保存 gate.id 后轮询等待 resolve。
    pub async fn create_gate(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        gate_kind: &str,
        correlation_id: &str,
        payload: Option<serde_json::Value>,
    ) -> Result<LifecycleGate, String> {
        let gate = LifecycleGate::open(
            run_id,
            Some(agent_id),
            None,
            gate_kind,
            correlation_id,
            payload,
        );

        self.repos
            .lifecycle_gate_repo
            .create(&gate)
            .await
            .map_err(|e| format!("create gate failed: {e}"))?;

        Ok(gate)
    }

    /// 轮询等待 gate 被 resolve，返回 resolve 后的 payload。
    /// 如果 gate 已经 resolved 则立即返回。
    pub async fn wait_for_gate(&self, gate_id: Uuid) -> Result<serde_json::Value, String> {
        let poll_interval = std::time::Duration::from_millis(500);
        let timeout = std::time::Duration::from_secs(300);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let gate = self
                .repos
                .lifecycle_gate_repo
                .get(gate_id)
                .await
                .map_err(|e| format!("poll gate failed: {e}"))?
                .ok_or_else(|| format!("gate not found: {gate_id}"))?;

            if !gate.is_open() {
                return Ok(gate.payload_json.unwrap_or(serde_json::Value::Null));
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(format!("gate {gate_id} timed out waiting for resolve"));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Resolve 一个 gate，写入 payload 并标记为 resolved。
    pub async fn resolve_gate(
        &self,
        gate_id: Uuid,
        payload: serde_json::Value,
        resolved_by: &str,
    ) -> Result<(), String> {
        let mut gate = self
            .repos
            .lifecycle_gate_repo
            .get(gate_id)
            .await
            .map_err(|e| format!("get gate failed: {e}"))?
            .ok_or_else(|| format!("gate not found: {gate_id}"))?;

        if !gate.is_open() {
            return Err(format!("gate {gate_id} is already resolved"));
        }

        gate.payload_json = Some(payload);
        gate.resolve(resolved_by);

        self.repos
            .lifecycle_gate_repo
            .update(&gate)
            .await
            .map_err(|e| format!("update gate failed: {e}"))?;

        Ok(())
    }
}
