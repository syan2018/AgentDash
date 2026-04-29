//! Session 上下文审计总线 —— Bundle/Fragment 产出与消费的可观测轨迹。
//!
//! 审计总线是 PRD Step 10 的核心：任何 `ContextFragment` 进入 `SessionContextBundle`
//! （由 compose_* 产出 / Hook 每轮合并 / Continuation 等）都会发一条 `ContextAuditEvent`，
//! 前端 Context Inspector 通过轮询 `/sessions/{id}/context/audit` 回放完整时间线。
//!
//! 首版采用进程内 `Arc<RwLock<HashMap<session_id, VecDeque<event>>>>` 环形缓冲，
//! 每 session 最多保留 `capacity_per_session` 条（默认 2000）；session_events 持久化
//! 稳定后再迁移。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

use agentdash_spi::{ContextFragment, FragmentScope, SessionContextBundle};
use uuid::Uuid;

/// 审计总线用于索引 session 的 key。
///
/// 与 `SessionContextBundle.session_id`（`Uuid`）不同，这里使用 `String`：
/// 因为生产环境 session ID 的形式是 `sess-<ms>-<shortuuid>`（见 `SessionHub`），
/// 不是纯 UUID。`Uuid` 仅用于 bundle 内部追踪。
pub type AuditSessionKey = String;

/// 审计事件的触发源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditTrigger {
    /// Owner bootstrap 路径产出的 Bundle（compose_owner_bootstrap）。
    SessionBootstrap,
    /// Compose 路径产出的 Bundle（story step / lifecycle / companion 等常规 rebuild）。
    ComposerRebuild,
    /// Hook 链路合并进 Bundle 的 fragment（来自 HookInjection → ContextFragment 转换）。
    ///
    /// `trigger` 字段承载具体的 HookTrigger 名称（如 `UserPromptSubmit` / `AfterTurn`），
    /// 避免反向依赖 agentdash-spi::hooks。
    HookInjection { trigger: String },
    /// Session plan fragments（来自 `build_session_plan_fragments`，当独立 emit 时使用）。
    SessionPlan,
    /// 能力层产出（session_capabilities / workflow capability 合流）。
    Capability,
    /// 消费侧：Bundle 被按 scope 过滤（供 title gen / summarizer / bridge replay 消费）。
    BundleFilter { scope: FragmentScope },
}

impl AuditTrigger {
    /// 序列化为稳定的字符串标签（DTO / 日志使用）。
    pub fn as_tag(&self) -> String {
        match self {
            AuditTrigger::SessionBootstrap => "session_bootstrap".to_string(),
            AuditTrigger::ComposerRebuild => "composer_rebuild".to_string(),
            AuditTrigger::HookInjection { trigger } => format!("hook:{trigger}"),
            AuditTrigger::SessionPlan => "session_plan".to_string(),
            AuditTrigger::Capability => "capability".to_string(),
            AuditTrigger::BundleFilter { scope } => format!("filter:{}", scope_tag(*scope)),
        }
    }
}

fn scope_tag(scope: FragmentScope) -> &'static str {
    match scope {
        FragmentScope::RuntimeAgent => "runtime_agent",
        FragmentScope::TitleGen => "title_gen",
        FragmentScope::Summarizer => "summarizer",
        FragmentScope::BridgeReplay => "bridge_replay",
        FragmentScope::Audit => "audit",
    }
}

/// 单条审计事件。
#[derive(Debug, Clone)]
pub struct ContextAuditEvent {
    pub event_id: Uuid,
    pub bundle_id: Uuid,
    /// session ID（与 `SessionHub` 分配的 `sess-<ms>-<short>` 形式一致）。
    pub session_id: AuditSessionKey,
    /// 内部 Bundle 追踪用的 UUID —— 可能是占位值，不等于 `session_id`。
    pub bundle_session_uuid: Uuid,
    pub at_ms: u64,
    pub trigger: AuditTrigger,
    pub fragment: ContextFragment,
    pub content_hash: u64,
}

/// 审计事件查询过滤器。
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub since_ms: Option<u64>,
    pub scope: Option<FragmentScope>,
    pub slot: Option<String>,
    pub source_prefix: Option<String>,
}

/// 审计总线 trait —— 进程内 / 远程 / 持久化实现按需替换。
pub trait ContextAuditBus: Send + Sync {
    fn emit(&self, event: ContextAuditEvent);
    fn query(&self, session_id: &str, filter: &AuditFilter) -> Vec<ContextAuditEvent>;
}

/// 进程内环形缓冲实现。
///
/// - 每个 session 持有独立 VecDeque，满员时按 FIFO 淘汰最旧事件；
/// - `capacity_per_session` 构造时确定（典型 2000）。
pub struct InMemoryContextAuditBus {
    capacity_per_session: usize,
    store: Arc<RwLock<HashMap<AuditSessionKey, VecDeque<ContextAuditEvent>>>>,
}

impl InMemoryContextAuditBus {
    pub fn new(capacity_per_session: usize) -> Self {
        Self {
            capacity_per_session: capacity_per_session.max(1),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryContextAuditBus {
    fn default() -> Self {
        Self::new(2000)
    }
}

impl ContextAuditBus for InMemoryContextAuditBus {
    fn emit(&self, event: ContextAuditEvent) {
        let mut guard = match self.store.write() {
            Ok(g) => g,
            Err(poisoned) => {
                tracing::warn!("context audit bus lock poisoned; 恢复并继续");
                poisoned.into_inner()
            }
        };
        let buf = guard
            .entry(event.session_id.clone())
            .or_insert_with(VecDeque::new);
        if buf.len() >= self.capacity_per_session {
            buf.pop_front();
        }
        buf.push_back(event);
    }

    fn query(&self, session_id: &str, filter: &AuditFilter) -> Vec<ContextAuditEvent> {
        let guard = match self.store.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(buf) = guard.get(session_id) else {
            return Vec::new();
        };
        buf.iter()
            .filter(|event| match filter.since_ms {
                Some(since) => event.at_ms >= since,
                None => true,
            })
            .filter(|event| match filter.scope {
                Some(scope) => event.fragment.scope.contains(scope),
                None => true,
            })
            .filter(|event| match filter.slot.as_deref() {
                Some(slot) => event.fragment.slot == slot,
                None => true,
            })
            .filter(|event| match filter.source_prefix.as_deref() {
                Some(prefix) => event.fragment.source.starts_with(prefix),
                None => true,
            })
            .cloned()
            .collect()
    }
}

/// 计算当前时间（毫秒，Unix epoch）。
fn now_millis_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_millis() as u64)
        .unwrap_or(0)
}

fn hash_content(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// 把 Bundle 内所有 fragment 发送为一批审计事件（共用 trigger / bundle_id / session_id）。
///
/// `session_key` 是 session 的外部 ID（SessionHub 分配的 `sess-<ms>-<short>`），
/// 与 `bundle.session_id`（内部 UUID，占位）不同。
pub fn emit_bundle_fragments(
    bus: &dyn ContextAuditBus,
    bundle: &SessionContextBundle,
    session_key: &str,
    trigger: AuditTrigger,
) {
    let at_ms = now_millis_u64();
    for fragment in &bundle.fragments {
        let content_hash = hash_content(&fragment.content);
        bus.emit(ContextAuditEvent {
            event_id: Uuid::new_v4(),
            bundle_id: bundle.bundle_id,
            session_id: session_key.to_string(),
            bundle_session_uuid: bundle.session_id,
            at_ms,
            trigger: trigger.clone(),
            fragment: fragment.clone(),
            content_hash,
        });
    }
}

/// 针对单个 fragment 发送审计事件。
pub fn emit_fragment(
    bus: &dyn ContextAuditBus,
    bundle_id: Uuid,
    session_key: &str,
    bundle_session_uuid: Uuid,
    trigger: AuditTrigger,
    fragment: &ContextFragment,
) {
    let content_hash = hash_content(&fragment.content);
    bus.emit(ContextAuditEvent {
        event_id: Uuid::new_v4(),
        bundle_id,
        session_id: session_key.to_string(),
        bundle_session_uuid,
        at_ms: now_millis_u64(),
        trigger,
        fragment: fragment.clone(),
        content_hash,
    });
}

/// `Arc<dyn ContextAuditBus>` 的共享别名，便于组件之间共享同一条总线。
pub type SharedContextAuditBus = Arc<dyn ContextAuditBus>;

/// 构造一个不做任何事的 bus 实现，用于单元测试 / 暂未接入的流程。
pub struct NoopContextAuditBus;

impl ContextAuditBus for NoopContextAuditBus {
    fn emit(&self, _event: ContextAuditEvent) {}
    fn query(&self, _session_id: &str, _filter: &AuditFilter) -> Vec<ContextAuditEvent> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::{ContextFragment, MergeStrategy, SessionContextBundle};

    fn frag(slot: &str, source: &str, content: &str) -> ContextFragment {
        ContextFragment {
            slot: slot.to_string(),
            label: format!("label_{slot}"),
            order: 10,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: source.to_string(),
            content: content.to_string(),
        }
    }

    const SESSION_KEY: &str = "sess-1700000000000-abcd1234";

    #[test]
    fn emit_and_query_roundtrip() {
        let bus = InMemoryContextAuditBus::new(100);
        let mut bundle = SessionContextBundle::new(Uuid::new_v4(), "task_start");
        bundle.upsert_by_slot(frag("task", "test:a", "alpha"));
        bundle.upsert_by_slot(frag("story", "test:b", "beta"));

        emit_bundle_fragments(&bus, &bundle, SESSION_KEY, AuditTrigger::SessionBootstrap);
        let events = bus.query(SESSION_KEY, &AuditFilter::default());
        assert_eq!(events.len(), 2);
        for event in &events {
            assert_eq!(event.bundle_id, bundle.bundle_id);
            assert_eq!(event.session_id, SESSION_KEY);
            assert_eq!(event.bundle_session_uuid, bundle.session_id);
            assert_eq!(event.trigger.as_tag(), "session_bootstrap");
        }
    }

    #[test]
    fn filter_by_slot_and_source_prefix() {
        let bus = InMemoryContextAuditBus::new(100);
        let mut bundle = SessionContextBundle::new(Uuid::new_v4(), "task_start");
        bundle.upsert_by_slot(frag("task", "legacy:session_plan", "alpha"));
        bundle.upsert_by_slot(frag("story", "hook:UserPromptSubmit", "beta"));

        emit_bundle_fragments(&bus, &bundle, SESSION_KEY, AuditTrigger::ComposerRebuild);
        let filter = AuditFilter {
            slot: Some("task".to_string()),
            ..AuditFilter::default()
        };
        let events = bus.query(SESSION_KEY, &filter);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].fragment.slot, "task");

        let filter = AuditFilter {
            source_prefix: Some("hook:".to_string()),
            ..AuditFilter::default()
        };
        let events = bus.query(SESSION_KEY, &filter);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].fragment.source, "hook:UserPromptSubmit");
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let bus = InMemoryContextAuditBus::new(2);
        for i in 0..5 {
            let mut bundle = SessionContextBundle::new(Uuid::new_v4(), "task_start");
            bundle.upsert_by_slot(frag("task", "test", &format!("v{i}")));
            emit_bundle_fragments(&bus, &bundle, SESSION_KEY, AuditTrigger::ComposerRebuild);
        }
        let events = bus.query(SESSION_KEY, &AuditFilter::default());
        assert_eq!(events.len(), 2);
        // 最后两条应为 v3, v4
        assert!(events[0].fragment.content.contains("v3"));
        assert!(events[1].fragment.content.contains("v4"));
    }

    #[test]
    fn filter_by_since_ms_skips_old_events() {
        let bus = InMemoryContextAuditBus::new(100);
        let mut bundle = SessionContextBundle::new(Uuid::new_v4(), "task_start");
        bundle.upsert_by_slot(frag("task", "test", "alpha"));
        emit_bundle_fragments(&bus, &bundle, SESSION_KEY, AuditTrigger::SessionBootstrap);

        let all = bus.query(SESSION_KEY, &AuditFilter::default());
        let first_ts = all[0].at_ms;

        let filter = AuditFilter {
            since_ms: Some(first_ts + 1_000_000),
            ..AuditFilter::default()
        };
        let filtered = bus.query(SESSION_KEY, &filter);
        assert!(filtered.is_empty());
    }

    #[test]
    fn noop_bus_is_always_empty() {
        let bus = NoopContextAuditBus;
        emit_fragment(
            &bus,
            Uuid::new_v4(),
            SESSION_KEY,
            Uuid::new_v4(),
            AuditTrigger::SessionPlan,
            &frag("task", "test", "alpha"),
        );
        assert!(bus.query(SESSION_KEY, &AuditFilter::default()).is_empty());
    }
}
