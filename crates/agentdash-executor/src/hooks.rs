use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookOwnerSummary {
    pub owner_type: String,
    pub owner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookContextFragment {
    pub slot: String,
    pub label: String,
    pub content: String,
    #[serde(default)]
    pub source_summary: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookConstraint {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub source_summary: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookPolicy {
    pub key: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HookDiagnosticEntry {
    pub code: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub source_summary: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SessionHookSnapshot {
    pub session_id: String,
    #[serde(default)]
    pub owners: Vec<HookOwnerSummary>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub context_fragments: Vec<HookContextFragment>,
    #[serde(default)]
    pub constraints: Vec<HookConstraint>,
    #[serde(default)]
    pub policies: Vec<HookPolicy>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookSessionRuntimeSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub snapshot: SessionHookSnapshot,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
}

pub struct HookSessionRuntime {
    session_id: String,
    provider: Arc<dyn ExecutionHookProvider>,
    snapshot: RwLock<SessionHookSnapshot>,
    diagnostics: RwLock<Vec<HookDiagnosticEntry>>,
    revision: AtomicU64,
}

impl std::fmt::Debug for HookSessionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookSessionRuntime")
            .field("session_id", &self.session_id)
            .field("revision", &self.revision())
            .field("snapshot", &self.snapshot())
            .field("diagnostics_count", &self.diagnostics().len())
            .finish()
    }
}

impl HookSessionRuntime {
    pub fn new(
        session_id: String,
        provider: Arc<dyn ExecutionHookProvider>,
        snapshot: SessionHookSnapshot,
    ) -> Self {
        let diagnostics = snapshot.diagnostics.clone();
        Self {
            session_id,
            provider,
            snapshot: RwLock::new(snapshot),
            diagnostics: RwLock::new(diagnostics),
            revision: AtomicU64::new(1),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn snapshot(&self) -> SessionHookSnapshot {
        self.snapshot
            .read()
            .expect("hook snapshot read lock poisoned")
            .clone()
    }

    pub fn diagnostics(&self) -> Vec<HookDiagnosticEntry> {
        self.diagnostics
            .read()
            .expect("hook diagnostics read lock poisoned")
            .clone()
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    pub fn runtime_snapshot(&self) -> HookSessionRuntimeSnapshot {
        HookSessionRuntimeSnapshot {
            session_id: self.session_id.clone(),
            revision: self.revision(),
            snapshot: self.snapshot(),
            diagnostics: self.diagnostics(),
        }
    }

    pub async fn refresh(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        let snapshot = self.provider.refresh_session_snapshot(query).await?;
        self.replace_snapshot(snapshot.clone());
        Ok(snapshot)
    }

    pub async fn evaluate(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError> {
        let resolution = self.provider.evaluate_hook(query).await?;
        self.append_diagnostics(resolution.diagnostics.clone());
        Ok(resolution)
    }

    pub fn replace_snapshot(&self, snapshot: SessionHookSnapshot) {
        {
            let mut guard = self
                .snapshot
                .write()
                .expect("hook snapshot write lock poisoned");
            *guard = snapshot.clone();
        }
        self.append_diagnostics(snapshot.diagnostics);
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    pub fn append_diagnostics<I>(&self, entries: I)
    where
        I: IntoIterator<Item = HookDiagnosticEntry>,
    {
        let mut guard = self
            .diagnostics
            .write()
            .expect("hook diagnostics write lock poisoned");
        for entry in entries {
            if guard.iter().any(|existing| {
                existing.code == entry.code
                    && existing.summary == entry.summary
                    && existing.detail == entry.detail
                    && existing.source_summary == entry.source_summary
            }) {
                continue;
            }
            guard.push(entry);
        }
    }
}

pub type SharedHookSessionRuntime = Arc<HookSessionRuntime>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SessionHookSnapshotQuery {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub owners: Vec<HookOwnerSummary>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SessionHookRefreshQuery {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    SessionStart,
    UserPromptSubmit,
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookEvaluationQuery {
    pub session_id: String,
    pub trigger: HookTrigger,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub snapshot: Option<SessionHookSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HookResolution {
    #[serde(default)]
    pub refresh_snapshot: bool,
    #[serde(default)]
    pub context_fragments: Vec<HookContextFragment>,
    #[serde(default)]
    pub constraints: Vec<HookConstraint>,
    #[serde(default)]
    pub policies: Vec<HookPolicy>,
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnosticEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewritten_tool_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum HookError {
    #[error("{0}")]
    Runtime(String),
}

#[async_trait]
pub trait ExecutionHookProvider: Send + Sync {
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, HookError>;

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError>;

    async fn evaluate_hook(&self, query: HookEvaluationQuery) -> Result<HookResolution, HookError>;
}

#[derive(Debug, Default)]
pub struct NoopExecutionHookProvider;

#[async_trait]
impl ExecutionHookProvider for NoopExecutionHookProvider {
    async fn load_session_snapshot(
        &self,
        query: SessionHookSnapshotQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }

    async fn refresh_session_snapshot(
        &self,
        query: SessionHookRefreshQuery,
    ) -> Result<SessionHookSnapshot, HookError> {
        Ok(SessionHookSnapshot {
            session_id: query.session_id,
            ..SessionHookSnapshot::default()
        })
    }

    async fn evaluate_hook(
        &self,
        _query: HookEvaluationQuery,
    ) -> Result<HookResolution, HookError> {
        Ok(HookResolution::default())
    }
}
