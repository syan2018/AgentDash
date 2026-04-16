# Routine 触发框架 — 定时 / Webhook / 可插拔事件源

> 状态：Planning
> 优先级：P1
> 前置：现有 `scheduling/cron_scheduler.rs` 基础设施、`AgentDashPlugin` 扩展点体系

## 背景

当前系统已实现 cron 定时调度的基础循环（5s tick + hot-reload），但存在两个问题：

1. **CronTriggerTarget 是 placeholder**：`AppCronTriggerTarget::trigger_agent_session()` 仅打日志，不创建 session，定时功能实际不可用（[cron_target.rs](crates/agentdash-api/src/bootstrap/cron_target.rs)）。
2. **调度配置藏在 Agent JSON config 里**：cron 表达式存储在 `Agent.base_config.scheduling` 或 `ProjectAgentLink.config_override.scheduling` 中，没有独立的领域实体，无法支持同一个 Agent 绑定多条不同触发规则。

参考 Claude Code Routines 的三路触发模型（Scheduled / HTTP Webhook / GitHub Webhook），我们需要将「触发 Agent 干活」提升为一等领域概念，并通过可插拔的 trigger provider 支持任意事件源扩展。

### 与 Hook 外部触发的关系

```
Routine  = 触发器 → 创建/路由 session → 执行 prompt     (session 的诞生)
Hook ExternalMessage = 外部事件 → 注入已有 session → 规则评估  (session 的治理)
```

两者互补：Routine 负责「什么时候启动 Agent」，Hook ExternalMessage（`03-30-hook-external-triggers`）负责「怎么往正在干活的 Agent 塞信息」。长活 session 场景（如 per-PR session 持续接收 push/comment 事件）需要两者协作——Routine 建 session，后续事件通过 ExternalMessage feed 回。

---

## 核心设计

### 领域模型

#### Routine 实体

```rust
/// 一等领域实体，project 级别
pub struct Routine {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    /// 每次触发时执行的 prompt 模板。
    /// 支持占位符插值：`{{trigger.payload.xxx}}`（由触发源填充）
    pub prompt_template: String,
    /// 绑定的执行 Agent
    pub agent_id: Uuid,
    /// 触发器配置（枚举，按类型存储不同字段）
    pub trigger_config: RoutineTriggerConfig,
    /// Session 生命周期策略
    pub session_strategy: SessionStrategy,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_fired_at: Option<DateTime<Utc>>,
}
```

#### 触发器配置

```rust
/// 触发器配置 — JSON tagged enum，存储在 routine 表的 trigger_config 列
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineTriggerConfig {
    /// 定时触发（cron 表达式）
    Scheduled {
        cron_expression: String,
        #[serde(default)]
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
        /// 如 `github:pull_request`、`gitlab:merge_request`、`jira:issue_updated`
        provider_key: String,
        /// 由 provider 定义的配置（event filter、subscription 等）
        provider_config: serde_json::Value,
    },
}
```

#### Session 策略

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStrategy {
    /// 每次触发新建独立 session
    Fresh,
    /// 复用 Project Agent 现有 session（follow-up prompt）
    Reuse,
    /// 按外部实体分配 session（如 per-PR、per-Issue）
    /// entity_key 由触发器 payload 中的指定字段提取
    PerEntity {
        /// payload 中用于提取 entity key 的 JSON path
        /// 如 `"pull_request.number"` → session affinity key = "PR#123"
        entity_key_path: String,
    },
}
```

#### 执行记录

```rust
/// 每次触发产生一条执行记录
pub struct RoutineExecution {
    pub id: Uuid,
    pub routine_id: Uuid,
    /// 触发来源标识：`"scheduled"` / `"webhook"` / `"github:pull_request.opened"` 等
    pub trigger_source: String,
    /// 触发时携带的 payload（webhook body / event payload）
    pub trigger_payload: Option<serde_json::Value>,
    /// 最终发送给 Agent 的实际 prompt（模板插值后）
    pub resolved_prompt: Option<String>,
    /// 关联的 session ID
    pub session_id: Option<String>,
    pub status: RoutineExecutionStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

pub enum RoutineExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,  // agent 仍在运行时跳过重入
}
```

---

### 触发器 SPI — 可插拔事件源

核心扩展点：`RoutineTriggerProvider` trait，遵循现有 `AgentDashPlugin` 体系。

```rust
// agentdash-spi 中新增

/// Routine 触发器提供者 — 插件通过实现此 trait 注册自定义事件源。
///
/// 生命周期：
/// 1. 插件注册时，宿主调用 `provider_key()` 获取唯一标识
/// 2. 用户创建 Routine 时选择 provider_key，宿主调用 `validate_config()` 校验配置
/// 3. 宿主调用 `start_listening()` 启动事件监听
/// 4. 事件到达时，provider 调用 `fire_callback` 通知宿主触发 Routine
/// 5. 服务关闭时，宿主调用 `stop_listening()`
#[async_trait]
pub trait RoutineTriggerProvider: Send + Sync {
    /// 唯一标识，格式 `plugin_name:trigger_type`
    /// 如 `"github:pull_request"`、`"gitlab:merge_request"`
    fn provider_key(&self) -> &str;

    /// 人类可读的显示名称
    fn display_name(&self) -> &str;

    /// 该 provider 支持的配置 schema（JSON Schema 子集），供前端渲染配置表单
    fn config_schema(&self) -> serde_json::Value;

    /// 校验用户提交的 provider_config 是否合法
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), String>;

    /// 启动对指定 Routine 的事件监听。
    /// `fire_callback` 是宿主提供的回调，provider 在事件到达时调用它。
    async fn start_listening(
        &self,
        routine_id: Uuid,
        config: &serde_json::Value,
        fire_callback: Arc<dyn RoutineFireCallback>,
    ) -> Result<(), String>;

    /// 停止对指定 Routine 的事件监听
    async fn stop_listening(&self, routine_id: Uuid) -> Result<(), String>;
}

/// 宿主提供给 provider 的回调接口
#[async_trait]
pub trait RoutineFireCallback: Send + Sync {
    /// 触发 Routine 执行。
    /// `trigger_source`: 事件标识（如 `"github:pull_request.opened"`）
    /// `payload`: 事件数据，将用于 prompt 模板插值和 entity_key 提取
    async fn fire(
        &self,
        trigger_source: String,
        payload: serde_json::Value,
    ) -> Result<Uuid, String>;  // 返回 RoutineExecution ID
}
```

#### Plugin 注册

在 `AgentDashPlugin` trait 中新增扩展点：

```rust
// agentdash-plugin-api/src/plugin.rs 新增

pub trait AgentDashPlugin: Send + Sync {
    // ... 现有方法 ...

    /// 注册 Routine 触发器提供者。
    ///
    /// 宿主会在运行时启动阶段完成 provider_key 冲突检测。
    fn routine_trigger_providers(&self) -> Vec<Arc<dyn RoutineTriggerProvider>> {
        vec![]
    }
}
```

---

### Prompt 模板插值（Tera）

使用 [Tera](https://keats.github.io/tera/)（Jinja2 语法）作为模板引擎。新增 `tera` crate 依赖（仅 `agentdash-application`）。

#### 模板上下文变量

```
固定变量：
  {{ trigger.source }}      — 触发来源（"scheduled" / "webhook" / provider_key）
  {{ trigger.timestamp }}   — 触发时间 ISO 8601
  {{ routine.name }}        — Routine 名称
  {{ routine.project_id }}  — 所属项目 ID

动态变量（来自 payload）：
  {{ trigger.payload }}           — 完整 payload（序列化为 JSON 字符串）
  {{ trigger.payload.xxx }}       — payload 中的指定字段（Tera 原生支持嵌套路径）
  {{ trigger.payload.xxx | default(value="N/A") }}  — 缺失时的默认值
```

#### 示例

```jinja2
Sentry 告警 {{ trigger.payload.alert_id }} 在生产环境触发。
错误信息：{{ trigger.payload.message }}
{% if trigger.payload.stacktrace %}
Stack trace：
{{ trigger.payload.stacktrace }}
{% endif %}
请分析根因并提出修复方案。
```

#### 错误处理

模板渲染失败（变量缺失且无 default 过滤器）时：
- 记录 RoutineExecution.error
- 不发送 prompt，status 置为 `Failed`
- 不静默吞错——模板有问题应该尽早暴露

---

### 调度器改造

现有 `CronScheduler` 从 Agent config 加载条目。改造后从 `Routine` 表加载：

```rust
// 改造前：扫描 Agent + ProjectAgentLink → 提取 scheduling config
// 改造后：查询 routine 表 WHERE trigger_config.type = 'scheduled' AND enabled = true

async fn load_cron_entries(repos: &RepositorySet) -> Result<Vec<CronEntry>, String> {
    let routines = repos.routine_repo
        .list_by_trigger_type("scheduled")
        .await?;

    routines.iter().filter_map(|r| {
        let RoutineTriggerConfig::Scheduled { cron_expression, .. } = &r.trigger_config else {
            return None;
        };
        // ... 解析 cron，构建 CronEntry（routine_id 替代 project_id+agent_id 组合键）
    }).collect()
}
```

`CronTriggerTarget` 泛化为内部的 `RoutineExecutor`，三种触发源共用同一条执行路径：
```
trigger → RoutineExecutor::execute(routine, payload)
       → 解析 session_strategy → 创建/复用 session
       → 插值 prompt_template → 发送 prompt
       → 记录 RoutineExecution
```

---

### HTTP Webhook 端点

每个 Webhook 类型的 Routine 拥有独立的 HTTP 端点：

```
POST /api/routines/{endpoint_id}/fire
Authorization: Bearer {token}
Content-Type: application/json

{
  "text": "可选的追加提示词，拼接到 prompt_template 末尾",
  "payload": {              // 可选，用于模板插值
    "alert_id": "SEN-4521",
    "severity": "critical"
  }
}

Response 200:
{
  "execution_id": "uuid",
  "session_id": "sess-xxx",    // 如果已创建
  "status": "pending"
}
```

- Token 在 Routine 创建时生成，bcrypt hash 存库，明文仅返回一次
- 端点 ID 自动生成（`trig_` + nanoid），不暴露 routine UUID
- 支持 rate limiting（project 级别，防止恶意调用）

---

### 与 Hook ExternalMessage 的协作（Phase 3+）

`SessionStrategy::PerEntity` 模式下，同一 entity 的后续触发事件应该 feed 回已有 session 而非新建。这需要与 Hook ExternalMessage 配合：

```
第一次触发（PR opened）：
  Routine → 新建 session（entity_key = "PR#123"）→ 执行 prompt

后续触发（PR push / comment）：
  Routine → 查找 entity_key="PR#123" 的已有 session
         → 如果 session 仍活跃：通过 HookTrigger::ExternalMessage 注入事件
         → 如果 session 已结束：新建 session（带历史摘要）
```

此能力依赖 `03-30-hook-external-triggers` 的 ExternalMessage 实现，作为后续 Phase 对接。

---

## 数据库

### routine 表

```sql
CREATE TABLE routine (
    id              UUID PRIMARY KEY,
    project_id      UUID NOT NULL REFERENCES project(id),
    name            TEXT NOT NULL,
    prompt_template TEXT NOT NULL,
    agent_id        UUID NOT NULL REFERENCES agent(id),
    trigger_config  JSONB NOT NULL,      -- RoutineTriggerConfig tagged enum
    session_strategy JSONB NOT NULL,     -- SessionStrategy
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_fired_at   TIMESTAMPTZ,

    UNIQUE(project_id, name)
);

CREATE INDEX idx_routine_project ON routine(project_id);
CREATE INDEX idx_routine_enabled ON routine(enabled) WHERE enabled = true;
```

### routine_execution 表

```sql
CREATE TABLE routine_execution (
    id              UUID PRIMARY KEY,
    routine_id      UUID NOT NULL REFERENCES routine(id),
    trigger_source  TEXT NOT NULL,
    trigger_payload JSONB,
    resolved_prompt TEXT,
    session_id      TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ,
    error           TEXT,

    -- PerEntity session affinity 查询
    entity_key      TEXT
);

CREATE INDEX idx_execution_routine ON routine_execution(routine_id);
CREATE INDEX idx_execution_status ON routine_execution(routine_id, status);
CREATE INDEX idx_execution_entity ON routine_execution(routine_id, entity_key)
    WHERE entity_key IS NOT NULL;
```

---

## 迁移策略

现有 `Agent.base_config.scheduling` 中的 cron 配置自动迁移为 Routine 实体：

```
对每个带 scheduling.cron_schedule 的 ProjectAgentLink：
  → 创建 Routine {
      project_id: link.project_id,
      name: "{agent_name}_cron_migration",
      prompt_template: "按照你的常规职责执行定时巡检。",  // 默认 prompt
      agent_id: link.agent_id,
      trigger_config: Scheduled { cron_expression: link.scheduling.cron_schedule },
      session_strategy: match link.scheduling.cron_session_mode {
          Reuse => SessionStrategy::Reuse,
          Fresh => SessionStrategy::Fresh,
      },
  }
```

迁移后可移除 `AgentSchedulingConfig` 及相关读取逻辑。迁移脚本作为数据库 migration 的一部分执行，不需要运行时向后兼容。

---

## REST API

```
# CRUD
POST   /api/projects/{project_id}/routines          创建 Routine
GET    /api/projects/{project_id}/routines          列表
GET    /api/routines/{routine_id}                    详情
PUT    /api/routines/{routine_id}                    更新
DELETE /api/routines/{routine_id}                    删除
PATCH  /api/routines/{routine_id}/enable             启用/禁用

# Webhook 专用
POST   /api/routines/{routine_id}/regenerate-token   重新生成 webhook token

# 触发端点（无需项目上下文，直接通过 endpoint_id 定位）
POST   /api/routine-triggers/{endpoint_id}/fire      外部触发

# 执行记录
GET    /api/routines/{routine_id}/executions         执行历史
GET    /api/routine-executions/{execution_id}        单条执行详情

# Provider 发现
GET    /api/routine-trigger-providers                 列出可用 trigger provider
GET    /api/routine-trigger-providers/{key}/schema    获取 provider 配置 schema
```

---

## 实施分 Phase

### Phase 0：领域模型 + Scheduled Trigger 端到端打通

**目标**：定时 Routine 端到端可用（创建 → cron 触发 → session 创建 → prompt 发送）

涉及 crate：
- `agentdash-domain`：Routine + RoutineExecution 实体
- `agentdash-spi`：`RoutineTriggerProvider` / `RoutineFireCallback` trait 定义
- `agentdash-infrastructure`：routine / routine_execution 表 + repository
- `agentdash-application`：`RoutineExecutor`（session 创建 + prompt 发送）、cron_scheduler 改造
- `agentdash-api`：Routine CRUD 路由、bootstrap 接入

附带：
- 数据库 migration（含现有 cron config 迁移脚本）
- 移除 `AgentSchedulingConfig` 及 cron_target.rs 中的占位实现

### Phase 1：Webhook Trigger

**目标**：外部系统通过 HTTP POST 触发 Routine

涉及 crate：
- `agentdash-domain`：`RoutineTriggerConfig::Webhook` 变体（已在 Phase 0 定义，此处实现逻辑）
- `agentdash-api`：`/api/routine-triggers/{endpoint_id}/fire` 端点、token 生成/验证
- `agentdash-application`：prompt 模板插值引擎

### Phase 2：Plugin Trigger Provider 框架

**目标**：第三方事件源可作为插件注入

涉及 crate：
- `agentdash-plugin-api`：`AgentDashPlugin::routine_trigger_providers()` 扩展点
- `agentdash-application`：`TriggerProviderRegistry`（provider 注册、生命周期管理）
- `agentdash-api`：provider 发现 API

附带：
- 内置参考实现（如 `builtin:cron_watcher` 或简单的 `builtin:manual`）

### Phase 3：PerEntity Session Affinity + ExternalMessage 对接

**目标**：长活 session 场景（如 per-PR）端到端可用

依赖：`03-30-hook-external-triggers` 的 ExternalMessage 实现

涉及 crate：
- `agentdash-application`：PerEntity session 路由逻辑、ExternalMessage feed-back
- `agentdash-infrastructure`：entity_key 索引查询

### Phase 4（可选）：前端 Routine 管理 UI

- Routine 列表 / 创建 / 编辑表单
- 执行历史时间线
- Provider 配置表单（根据 config_schema 动态渲染）

---

## 非目标

- **Routine 内部的 Agent 行为治理**：由 Hook 引擎负责，不在本任务范围
- **GitHub/GitLab 特定 trigger provider 实现**：作为独立插件任务，本任务只提供框架
- **Routine 执行的计费/限额**：后续独立任务
- **多租户隔离**：当前项目级即可

---

## 已决策

- **D1: 模板引擎 → Tera**：引入 `tera` crate，获得 Jinja2 语法（条件/循环/过滤器）。Rhai 虽已有但学习曲线高且不适合模板场景。
- **D2: Token 策略 → 单 token + regenerate**：每个 Webhook Routine 仅一个 active token，regenerate 时旧 token 立即失效。简单直接，后续有需要再扩展。
- **D3: 执行记录 → 不主动清理**：RoutineExecution 暂不设自动清理策略，按需后加。执行频率不高的情况下短期内不是问题。
- **D4: Session 创建 → SessionHub 直接创建**：RoutineExecutor 通过 `session_hub.create_session()` + `session_hub.start_prompt()` 创建顶层独立会话。Companion 层是嵌套会话专用，Routine 不经过它。Reuse 模式下找到已有 session 后直接 `start_prompt()` 发 follow-up。

## 待讨论

- [ ] PerEntity 的 entity_key 冲突处理：不同 Routine 产生相同 entity_key 时如何隔离？（routine_id + entity_key 组合即可）
- [ ] 是否需要 Routine 级别的 "暂停" 状态（区别于 enabled=false 的永久禁用）？

---

## 关联任务

- `03-30-hook-external-triggers` — Hook 外部触发（ExternalMessage），Phase 3 依赖
- `04-12-plugin-extension-api` — 插件扩展 API，本任务在其基础上新增 trigger provider 扩展点
- `04-08-hook-event-coverage` — Hook 事件覆盖扩展

## 参考

- Claude Code Routines（三路触发：Scheduled / HTTP / GitHub Webhook）
- 现有 cron 调度器：`crates/agentdash-application/src/scheduling/`
- 现有 Plugin API：`crates/agentdash-plugin-api/src/plugin.rs`
- Hook SPI：`crates/agentdash-spi/src/hooks.rs`
