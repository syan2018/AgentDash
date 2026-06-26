use agentdash_diagnostics::{diag, Subsystem};
use std::collections::{HashMap, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

use agentdash_spi::{ContextFragment, FragmentScope, SessionContextBundle};
use uuid::Uuid;

pub(crate) struct Contribution {
    pub fragments: Vec<ContextFragment>,
}

impl Contribution {
    pub(crate) fn fragments_only(fragments: Vec<ContextFragment>) -> Self {
        Self { fragments }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditTrigger {
    SessionBootstrap,
    ComposerRebuild,
    HookInjection { trigger: String },
    SessionPlan,
    Capability,
    BundleFilter { scope: FragmentScope },
}

impl AuditTrigger {
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

#[derive(Debug, Clone)]
pub struct ContextAuditEvent {
    pub event_id: Uuid,
    pub bundle_id: Uuid,
    pub session_id: String,
    pub bundle_session_uuid: Uuid,
    pub at_ms: u64,
    pub trigger: AuditTrigger,
    pub fragment: ContextFragment,
    pub content_hash: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub since_ms: Option<u64>,
    pub scope: Option<FragmentScope>,
    pub slot: Option<String>,
    pub source_prefix: Option<String>,
}

pub trait ContextAuditBus: Send + Sync {
    fn emit(&self, event: ContextAuditEvent);
    fn query(&self, session_id: &str, filter: &AuditFilter) -> Vec<ContextAuditEvent>;
}

pub struct InMemoryContextAuditBus {
    capacity_per_session: usize,
    store: Arc<RwLock<HashMap<String, VecDeque<ContextAuditEvent>>>>,
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
            Ok(guard) => guard,
            Err(poisoned) => {
                diag!(Warn, Subsystem::AgentRun,
        "context audit bus lock poisoned; 恢复并继续");
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
            Ok(guard) => guard,
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

pub type SharedContextAuditBus = Arc<dyn ContextAuditBus>;

pub fn emit_bundle_fragments(
    bus: &dyn ContextAuditBus,
    bundle: &SessionContextBundle,
    session_key: &str,
    trigger: AuditTrigger,
) {
    let at_ms = now_millis_u64();
    for fragment in bundle.iter_fragments() {
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

pub struct NoopContextAuditBus;

impl ContextAuditBus for NoopContextAuditBus {
    fn emit(&self, _event: ContextAuditEvent) {}

    fn query(&self, _session_id: &str, _filter: &AuditFilter) -> Vec<ContextAuditEvent> {
        Vec::new()
    }
}
