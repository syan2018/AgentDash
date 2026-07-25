use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workflow::AgentRuntimeRefs;

/// Routine — 一等领域实体，项目级别的 Agent 触发规则
///
/// 将「什么时候启动 Agent 干活」提升为独立的领域概念，
/// 支持定时（cron）、HTTP Webhook、插件事件源三类触发方式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    /// 每次触发时执行的 prompt 模板（Tera/Jinja2 语法）
    pub prompt_template: String,
    /// 绑定的 Project Agent
    pub project_agent_id: Uuid,
    /// 触发器配置（按类型存储不同字段）
    pub trigger_config: RoutineTriggerConfig,
    /// Dispatch 生命周期策略
    pub dispatch_strategy: DispatchStrategy,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<DateTime<Utc>>,
}

impl Routine {
    pub fn new(
        project_id: Uuid,
        name: impl Into<String>,
        prompt_template: impl Into<String>,
        project_agent_id: Uuid,
        trigger_config: RoutineTriggerConfig,
        dispatch_strategy: DispatchStrategy,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            prompt_template: prompt_template.into(),
            project_agent_id,
            trigger_config,
            dispatch_strategy,
            enabled: true,
            created_at: now,
            updated_at: now,
            last_fired_at: None,
        }
    }
}

/// 触发器配置 — JSON tagged enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineTriggerConfig {
    /// 定时触发（cron 表达式）
    Scheduled {
        cron_expression: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timezone: Option<String>,
    },
    /// HTTP Webhook 触发
    Webhook {
        /// 触发端点路径后缀（自动生成，形如 `trig_xxxx`）
        endpoint_id: String,
        /// Bearer token 的 bcrypt hash
        auth_token_hash: String,
    },
    /// 插件提供的自定义触发器
    Plugin {
        /// 触发器类型标识，格式 `plugin_name:trigger_type`
        provider_key: String,
        /// 由 provider 定义的配置
        #[serde(default)]
        provider_config: serde_json::Value,
    },
}

/// Dispatch 生命周期策略
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum DispatchStrategy {
    /// 每次触发新建独立 dispatch 目标
    #[default]
    Fresh,
    /// 复用 Project Agent 现有 dispatch 目标
    Reuse,
    /// 按外部实体分配 dispatch 目标（如 per-PR、per-Issue）
    PerEntity {
        /// payload 中用于提取 entity key 的 JSON path
        entity_key_path: String,
    },
}

/// Dispatch 结果引用——记录 LifecycleRun / LifecycleAgent / AgentFrame 的稳定锚点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineDispatchRefs {
    pub runtime_refs: AgentRuntimeRefs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_handoff_refs: Option<RoutineInputHandoffRefs>,
}

impl RoutineDispatchRefs {
    pub fn new(runtime_refs: AgentRuntimeRefs) -> Self {
        Self {
            runtime_refs,
            input_handoff_refs: None,
        }
    }

    pub fn with_input_handoff_refs(mut self, input_handoff_refs: RoutineInputHandoffRefs) -> Self {
        self.input_handoff_refs = Some(input_handoff_refs);
        self
    }

    pub fn run_id(&self) -> Uuid {
        self.runtime_refs.run_ref
    }

    pub fn agent_id(&self) -> Uuid {
        self.runtime_refs.agent_ref
    }

    pub fn frame_id(&self) -> Uuid {
        self.runtime_refs.frame_ref
    }

    pub fn orchestration_id(&self) -> Option<Uuid> {
        self.runtime_refs.orchestration_ref()
    }

    pub fn node_path(&self) -> Option<&str> {
        self.runtime_refs.node_path()
    }

    pub fn node_attempt(&self) -> Option<u32> {
        self.runtime_refs.node_attempt()
    }
}

/// Input handoff refs for Routine executions that reuse an existing AgentRun.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineInputHandoffRefs {
    pub handoff_id: Uuid,
    pub client_command_id: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_operation_id: Option<String>,
}

/// 每次触发产生的执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineExecution {
    pub id: Uuid,
    pub routine_id: Uuid,
    /// 触发来源标识（`"scheduled"` / `"webhook"` / `"github:pull_request.opened"` 等）
    pub trigger_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_prompt: Option<String>,
    /// Dispatch 目标锚点——dispatch 成功后记录 run/agent/frame refs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_refs: Option<RoutineDispatchRefs>,
    pub status: RoutineExecutionStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// PerEntity dispatch affinity key
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_key: Option<String>,
}

impl RoutineExecution {
    pub fn new(routine_id: Uuid, trigger_source: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            routine_id,
            trigger_source: trigger_source.into(),
            trigger_payload: None,
            resolved_prompt: None,
            dispatch_refs: None,
            status: RoutineExecutionStatus::Pending,
            started_at: Utc::now(),
            completed_at: None,
            error: None,
            entity_key: None,
        }
    }

    /// 输入已交给目标 Agent，并记录可恢复的交接收据。
    /// `Dispatched` 表示输入已被 Agent 接受，真正 terminal 从 Agent observation 派生。
    pub fn mark_dispatched(&mut self, refs: RoutineDispatchRefs, resolved_prompt: String) {
        self.dispatch_refs = Some(refs);
        self.resolved_prompt = Some(resolved_prompt);
        self.status = RoutineExecutionStatus::Dispatched;
        self.error = None;
    }

    /// Product graph 与 frozen input intent 已持久化，可以在进程重启后从相同
    /// AgentRun target 继续完成输入交接。
    pub fn mark_dispatch_prepared(&mut self, refs: RoutineDispatchRefs, resolved_prompt: String) {
        self.dispatch_refs = Some(refs);
        self.resolved_prompt = Some(resolved_prompt);
        self.status = RoutineExecutionStatus::Pending;
        self.error = None;
    }

    pub fn mark_recovery_pending(&mut self, error: impl Into<String>) {
        self.status = RoutineExecutionStatus::Pending;
        self.error = Some(error.into());
    }

    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = RoutineExecutionStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_skipped(&mut self, reason: impl Into<String>) {
        self.status = RoutineExecutionStatus::Skipped;
        self.error = Some(reason.into());
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_terminal(&mut self, status: RoutineExecutionStatus, detail: Option<String>) {
        debug_assert!(matches!(
            status,
            RoutineExecutionStatus::Completed
                | RoutineExecutionStatus::Failed
                | RoutineExecutionStatus::Interrupted
        ));
        self.status = status;
        self.error = detail;
        self.completed_at = Some(Utc::now());
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineExecutionStatus {
    #[default]
    Pending,
    /// 输入已成功提交给目标 Agent。
    /// 真正的 terminal status 从 Agent observation 派生。
    Dispatched,
    Completed,
    Failed,
    Interrupted,
    /// Agent 仍在运行时跳过重入
    Skipped,
}
