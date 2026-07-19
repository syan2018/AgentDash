use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use agentdash_agent::AgentToolError;
use agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry;
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository;
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleGateRepository,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::sources::{
    exec_item_from_terminal, gate_belongs_to_scope, gate_item_from_gate, mailbox_belongs_to_scope,
    mailbox_item_from_message, mailbox_message_is_wait_relevant, terminal_belongs_to_scope,
};
use super::types::{
    ResolvedWaitScope, WAIT_POLL_INTERVAL_MS, WAIT_TOOL_TIMEOUT_MS_DEFAULT,
    WAIT_TOOL_TIMEOUT_MS_MAX, WaitActivityItem, WaitActivityRequest, WaitActivityResult,
    WaitToolContext,
};
use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;

#[derive(Clone)]
pub struct WaitActivityRepositories {
    pub lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub agent_frame_repo: Arc<dyn AgentFrameRepository>,
    pub agent_run_runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    pub mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
}

#[derive(Clone)]
pub struct WaitActivityDeps {
    pub repositories: WaitActivityRepositories,
    pub terminal_registry: Arc<AgentRunTerminalRegistry>,
}

#[derive(Clone)]
pub struct WaitActivityService {
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    agent_run_runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
    terminal_registry: Arc<AgentRunTerminalRegistry>,
}

impl WaitActivityService {
    pub fn new(deps: WaitActivityDeps) -> Self {
        Self::from_repository_ports(deps.repositories, deps.terminal_registry)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_repositories(
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        agent_run_runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
        lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
        mailbox_repo: Arc<dyn AgentRunMailboxRepository>,
        terminal_registry: Arc<AgentRunTerminalRegistry>,
    ) -> Self {
        Self {
            lifecycle_agent_repo,
            agent_frame_repo,
            agent_run_runtime_binding_repo,
            lifecycle_gate_repo,
            mailbox_repo,
            terminal_registry,
        }
    }

    pub fn from_repository_ports(
        repositories: WaitActivityRepositories,
        terminal_registry: Arc<AgentRunTerminalRegistry>,
    ) -> Self {
        Self {
            lifecycle_agent_repo: repositories.lifecycle_agent_repo,
            agent_frame_repo: repositories.agent_frame_repo,
            agent_run_runtime_binding_repo: repositories.agent_run_runtime_binding_repo,
            lifecycle_gate_repo: repositories.lifecycle_gate_repo,
            mailbox_repo: repositories.mailbox_repo,
            terminal_registry,
        }
    }

    pub async fn wait(
        &self,
        context: WaitToolContext,
        request: WaitActivityRequest,
        cancel: CancellationToken,
    ) -> Result<WaitActivityResult, AgentToolError> {
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(WAIT_TOOL_TIMEOUT_MS_DEFAULT)
            .min(WAIT_TOOL_TIMEOUT_MS_MAX);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        let mut observed_activity_refs = request
            .normalized_activity_refs()
            .into_iter()
            .collect::<BTreeSet<_>>();

        loop {
            let scope = self.resolve_scope(&context).await?;
            let mut items = self
                .collect_items(&scope, &request, &observed_activity_refs)
                .await?;
            observed_activity_refs.extend(items.iter().map(|item| item.activity_ref.clone()));
            if let Some(after_cursor_ms) = request.after_cursor_ms() {
                items.retain(|item| item.updated_at_ms > after_cursor_ms);
            }
            truncate_items(&mut items, request.max_items());
            let ready = items.iter().any(WaitActivityItem::is_ready);
            if ready {
                return Ok(WaitActivityResult::ready(items));
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(WaitActivityResult::timed_out(items));
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let sleep_for = remaining.min(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(AgentToolError::ExecutionFailed("wait 被取消".to_string()));
                }
                _ = tokio::time::sleep(sleep_for) => {}
            }
        }
    }

    pub(crate) async fn resolve_scope(
        &self,
        context: &WaitToolContext,
    ) -> Result<ResolvedWaitScope, AgentToolError> {
        let mut scope = ResolvedWaitScope {
            runtime_thread_id: context.runtime_thread_id.clone(),
            run_id: context.owner.map(|owner| owner.run_id),
            agent_id: context.owner.map(|owner| owner.agent_id),
            frame_id: context.owner.map(|owner| owner.frame_id),
        };
        if context.owner.is_some() {
            return Ok(scope);
        }
        let Some(runtime_thread_id) = context.runtime_thread_id.as_ref() else {
            return Ok(scope);
        };

        let resolved = resolve_current_frame_from_delivery_trace_ref(
            runtime_thread_id.as_str(),
            self.agent_run_runtime_binding_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
        )
        .await
        .map_err(|error| {
            AgentToolError::ExecutionFailed(format!("wait 解析 AgentRun owner 失败: {error}"))
        })?;
        if let Some((_binding, agent, frame)) = resolved {
            scope.run_id = Some(agent.run_id);
            scope.agent_id = Some(agent.id);
            scope.frame_id = Some(frame.id);
        }
        Ok(scope)
    }

    pub(crate) async fn collect_items(
        &self,
        scope: &ResolvedWaitScope,
        request: &WaitActivityRequest,
        observed_activity_refs: &BTreeSet<String>,
    ) -> Result<Vec<WaitActivityItem>, AgentToolError> {
        let filters = request.normalized_kinds();
        let explicit_refs = request.normalized_activity_refs();
        let mut items = Vec::new();

        if explicit_refs.is_empty() {
            self.collect_scope_exec_items(scope, &filters, &mut items);
            self.collect_scope_gate_items(scope, &filters, &mut items)
                .await?;
            self.collect_scope_mailbox_items(scope, &filters, &mut items)
                .await?;
        } else {
            for activity_ref in &explicit_refs {
                self.collect_explicit_ref(scope, activity_ref, &filters, &mut items)
                    .await?;
            }
        }
        for activity_ref in observed_activity_refs {
            if items.iter().any(|item| item.activity_ref == *activity_ref) {
                continue;
            }
            self.collect_explicit_ref(scope, activity_ref, &filters, &mut items)
                .await?;
        }

        items.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| left.activity_ref.cmp(&right.activity_ref))
        });
        Ok(items)
    }

    async fn collect_explicit_ref(
        &self,
        scope: &ResolvedWaitScope,
        activity_ref: &str,
        filters: &BTreeSet<String>,
        items: &mut Vec<WaitActivityItem>,
    ) -> Result<(), AgentToolError> {
        if accepts_kind(filters, "exec")
            && let Some(terminal) = self.terminal_registry.get_terminal(activity_ref)
        {
            if !terminal_belongs_to_scope(&terminal, scope) {
                return Ok(());
            }
            items.push(exec_item_from_terminal(&terminal));
            return Ok(());
        }
        if let Ok(uuid) = Uuid::parse_str(activity_ref) {
            if accepts_any_kind(filters, &["human", "subagent", "companion", "workflow"])
                && let Some(gate) = self
                    .lifecycle_gate_repo
                    .get(uuid)
                    .await
                    .map_err(domain_error("wait 查询 LifecycleGate 失败"))?
            {
                if !gate_belongs_to_scope(&gate, scope) {
                    return Ok(());
                }
                let item = gate_item_from_gate(&gate);
                if accepts_kind(filters, &item.kind) {
                    items.push(item);
                    return Ok(());
                }
            }
            if accepts_kind(filters, "mailbox")
                && let Some(message) = self
                    .mailbox_repo
                    .get_message(uuid)
                    .await
                    .map_err(domain_error("wait 查询 mailbox message 失败"))?
            {
                if !mailbox_belongs_to_scope(&message, scope) {
                    return Ok(());
                }
                items.push(mailbox_item_from_message(&message));
            }
        }
        Ok(())
    }

    fn collect_scope_exec_items(
        &self,
        scope: &ResolvedWaitScope,
        filters: &BTreeSet<String>,
        items: &mut Vec<WaitActivityItem>,
    ) {
        if !accepts_kind(filters, "exec") {
            return;
        }
        let (Some(run_id), Some(agent_id)) = (scope.run_id, scope.agent_id) else {
            return;
        };
        items.extend(
            self.terminal_registry
                .list_terminals(&run_id.to_string(), &agent_id.to_string())
                .into_iter()
                .map(|terminal| exec_item_from_terminal(&terminal)),
        );
    }

    async fn collect_scope_gate_items(
        &self,
        scope: &ResolvedWaitScope,
        filters: &BTreeSet<String>,
        items: &mut Vec<WaitActivityItem>,
    ) -> Result<(), AgentToolError> {
        if !accepts_any_kind(filters, &["human", "subagent", "companion", "workflow"]) {
            return Ok(());
        }
        let Some(agent_id) = scope.agent_id else {
            return Ok(());
        };
        let gates = self
            .lifecycle_gate_repo
            .list_open_for_agent(agent_id)
            .await
            .map_err(domain_error("wait 查询 open LifecycleGate 失败"))?;
        items.extend(
            gates
                .into_iter()
                .map(|gate| gate_item_from_gate(&gate))
                .filter(|item| accepts_kind(filters, &item.kind)),
        );
        Ok(())
    }

    async fn collect_scope_mailbox_items(
        &self,
        scope: &ResolvedWaitScope,
        filters: &BTreeSet<String>,
        items: &mut Vec<WaitActivityItem>,
    ) -> Result<(), AgentToolError> {
        if !accepts_kind(filters, "mailbox") {
            return Ok(());
        }
        let (Some(run_id), Some(agent_id)) = (scope.run_id, scope.agent_id) else {
            return Ok(());
        };
        let messages = self
            .mailbox_repo
            .list_messages(run_id, agent_id)
            .await
            .map_err(domain_error("wait 查询 mailbox message 失败"))?;
        items.extend(
            messages
                .into_iter()
                .filter(mailbox_message_is_wait_relevant)
                .map(|message| mailbox_item_from_message(&message)),
        );
        Ok(())
    }

    pub async fn wait_for_lifecycle_gate_payload(
        &self,
        context: WaitToolContext,
        gate_id: Uuid,
        cancel: CancellationToken,
        timeout: Duration,
    ) -> Result<Option<Value>, AgentToolError> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let timeout_ms = duration_millis_u64(remaining).min(WAIT_TOOL_TIMEOUT_MS_MAX);
            let result = self
                .wait(
                    context.clone(),
                    WaitActivityRequest {
                        activity_refs: vec![gate_id.to_string()],
                        kinds: Vec::new(),
                        timeout_ms: Some(timeout_ms),
                        max_items: Some(1),
                        after_cursor: None,
                    },
                    cancel.clone(),
                )
                .await?;

            if !result.timed_out {
                let gate = self
                    .lifecycle_gate_repo
                    .get(gate_id)
                    .await
                    .map_err(domain_error("wait 查询 LifecycleGate 结果失败"))?
                    .ok_or_else(|| {
                        AgentToolError::ExecutionFailed(format!("gate {gate_id} 不存在"))
                    })?;
                if !gate.is_open() {
                    return Ok(Some(gate.payload_json.unwrap_or_else(|| json!({}))));
                }
            }

            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }
}

fn domain_error(
    context: &'static str,
) -> impl FnOnce(agentdash_domain::DomainError) -> AgentToolError {
    move |error| AgentToolError::ExecutionFailed(format!("{context}: {error}"))
}

fn accepts_kind(filters: &BTreeSet<String>, kind: &str) -> bool {
    filters.is_empty() || filters.contains(kind)
}

fn accepts_any_kind(filters: &BTreeSet<String>, kinds: &[&str]) -> bool {
    filters.is_empty() || kinds.iter().any(|kind| filters.contains(*kind))
}

fn truncate_items(items: &mut Vec<WaitActivityItem>, max_items: usize) {
    if items.len() > max_items {
        items.truncate(max_items);
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
