# 设计：Tauri 桌面端统一架构与 Local Runtime 控制台

## 设计原则

- Desktop 是 AgentDash 的本机能力控制台，不只是 WebView 包壳。
- `agentdash-local` 是 runtime library，CLI 和 Tauri 都只是宿主。
- 云端 API/relay 仍是跨设备协作事实源；desktop command 只管理本机能力与本机即时状态。
- Runtime health 是持久化业务资源，BackendRegistry 是在线 transport registry。
- Story/Task/SessionHub/VFS/Runtime Gateway 的现有边界不被 multica 的 Issue/task queue/worktree 模型替换。
- 当前项目处于预研期，不做兼容性或回退方案设计；schema 与 API 按长期正确形态迁移。

## 总体架构

```text
-----------------------------+
| packages/app-web           |
| Web app shell              |
+-------------+---------------+
              |
              v
+-----------------------------+       +-----------------------------+
| packages/views              |       | packages/app-tauri          |
| Dashboard / Story / Task    |<------| Tauri renderer shell        |
| Local Settings views        |       | routes + desktop adapters   |
+-------------+---------------+       +--------------+--------------+
              |                                      |
              v                                      v
+-----------------------------+       +-----------------------------+
| packages/core               |       | crates/agentdash-local-tauri |
| API clients / query keys    |       | Tauri commands / runtime mgr |
| realtime sync / types       |       +--------------+--------------+
+-------------+---------------+                      |
              |                                      v
              |                       +-----------------------------+
              |                       | crates/agentdash-local       |
              |                       | library API + optional bin   |
              |                       +--------------+--------------+
              |                                      |
              v                                      v
+-----------------------------+       +-----------------------------+
| crates/agentdash-api        |<----->| local runtime transport      |
| Runtime API / relay / SSE   |       | WS relay / SessionHub / MCP  |
+-----------------------------+       +-----------------------------+
```

## 后端边界

### `agentdash-local` library

新增 library 层后，`main.rs` 不再直接组装所有运行时对象。建议核心结构：

```rust
pub struct LocalRuntimeConfig {
    pub cloud_url: Option<String>,
    pub token: Option<String>,
    pub backend_id: String,
    pub name: String,
    pub accessible_roots: Vec<PathBuf>,
    pub executor_enabled: bool,
    pub profile: LocalProfile,
}

pub struct LocalRuntimeHandle {
    pub backend_id: String,
    pub status_rx: watch::Receiver<LocalRuntimeStatus>,
    pub log_rx: broadcast::Receiver<LocalLogEvent>,
}

pub struct LocalRuntimeManager;

impl LocalRuntimeManager {
    pub async fn start(&self, config: LocalRuntimeConfig) -> Result<LocalRuntimeHandle>;
    pub async fn stop(&self, reason: StopReason) -> Result<()>;
    pub async fn restart(&self, reason: RestartReason) -> Result<RestartOutcome>;
    pub async fn snapshot(&self) -> LocalRuntimeSnapshot;
}
```

`ws_client::run` 需要拆成可取消的 task，而不是无限 loop 占据调用方。重连、注册、capability 更新、SessionHub 恢复、MCP manager 都通过 handle 汇报状态。

### `agentdash-local-tauri`

新增薄 Tauri crate，职责只包括：

- Tauri app bootstrap。
- runtime manager 生命周期。
- command：`runtime_start`、`runtime_stop`、`runtime_restart`、`runtime_snapshot`、`logs_tail`、`mcp_list`、`mcp_update`、`mcp_probe`、`roots_list`、`roots_update`。
- local profile/config 文件读写。
- 日志脱敏和 ring buffer。

业务规则仍尽量放在 `agentdash-local` library 或 application/domain 层，避免 Tauri command 变成第二套后端。

### cloud runtime health

新增持久化 runtime/backend health 模型。建议最小字段：

```text
runtime_id / backend_id
project_id 或 owner scope
profile_id
name
status: online | offline | starting | degraded | stopping | error
last_seen_at
connected_at
disconnected_at
disconnect_reason
version
capabilities jsonb
accessible_roots jsonb
device jsonb
created_at / updated_at
```

`BackendRegistry` 注册成功时 upsert health 为 online，断开时写 offline/degraded。后续 sweeper 用 `last_seen_at` 做二次校验，学习 multica 的 stale predicate 思路，但状态流转仍按 AgentDash relay 实现。

### execution attempt/message log 投影

建议新增“投影表”，不是替代 session event：

```text
execution_attempts:
  id
  story_id
  task_id nullable
  session_id
  lifecycle_run_id nullable
  lifecycle_step_key nullable
  runtime_id/backend_id nullable
  status
  started_at / completed_at
  executor_session_id nullable
  failure_reason nullable
  usage jsonb
  workdir_ref nullable

execution_messages:
  id
  attempt_id
  seq
  kind
  summary
  payload_ref nullable
  created_at
```

投影来源是 SessionHub event、LifecycleRun step state 和 relay session events。不得把 Backbone 全量事件复制一份；message 只保留用户需要浏览/过滤的摘要与引用。

## 前端边界

### 包拆分顺序

当前 `frontend` 先作为源目录存在，拆分应按使用压力推进：

1. `packages/ui`：移动纯 UI 组件和样式 token。
2. `packages/core`：移动 API client、types、query keys、realtime sync、宿主无关 stores。
3. `packages/views`：移动 Dashboard/Story/Task/Workflow/Local Settings 等业务页面。
4. `packages/app-web`：Web app shell，替代当前 `frontend` 入口。
5. `packages/app-tauri`：Desktop renderer shell，注入 desktop adapter。

`pnpm-workspace.yaml` 改为包含 `packages/*`，`pnpm dev` 继续作为联合调试入口，并扩展 desktop dev 命令。

### server state 规范

借鉴 multica 的 Query discipline，但不强行一次性引入全量迁移：

- runtime health、backend list、MCP server list、execution attempts 作为第一批 query key。
- Zustand 保留 current project、tabs、draft、filters、local panel state。
- SSE/NDJSON 或后续 business event 只负责 invalidation/patch，不在多个 store 中重复保存同一服务端事实。

### Local 设置页信息源

Local UI 同时消费两类状态：

- Cloud authority：后端 runtime health、BackendRegistry 合并 API、project/workspace/session 数据。
- Local immediate：Tauri command 返回的 local runtime snapshot、日志、profile、active work、MCP probe 结果。

UI 必须在字段层标注来源或合并规则。例如 backend online 以 cloud 为准，启动中/日志/本机进程 PID 以来自 desktop local snapshot 为准。

## multica 借鉴与改写

| multica 机制 | AgentDash 采用方式 |
| --- | --- |
| desktop daemon manager | 改写为 Tauri runtime manager，管理 `agentdash-local` library，而不是 Electron daemon process |
| runtime heartbeat / last_seen / stale sweeper | 引入 cloud runtime health 资源，保留 relay WS 主动连接模型 |
| task_message / task_usage | 转译为 execution attempt/message log 投影，不替代 SessionHub event |
| core/views/ui/app 分层 | 渐进拆包，先服务 desktop 复用，不一次性搬空 |
| local profile/token sync | 引入 desktop-managed profile，与手动 CLI profile 分离 |
| version mismatch restart defer | 引入 active work gate，基于 session/terminal/MCP/materialization 活跃统计决定是否可重启 |
| repo/worktree GC | 只学习 env root lifecycle/GC meta；不绕过 VFS materialization |

## 数据库与迁移

- 所有新增 runtime health、attempt/message log、profile 相关表必须有 migration。
- 当前项目未上线，不做旧 API/旧字段兼容双写。
- migration 应遵守 backend database guidelines：幂等、明确约束、索引覆盖列表查询。
- `state_changes` 可作为 project-level 全局游标索引继续存在；不要把新业务事件直接塞进 handler 内临时写入，应通过 projector/outbox 方向演进。

## 安全与隐私

- Desktop logs 默认脱敏 token、Authorization、路径中可能暴露用户名的敏感片段、prompt 内容长文本。
- Local 设置页只允许操作 desktop-managed profile 下的 runtime。
- accessible roots 更新必须规范化绝对路径，并与 VFS/relay path safety 保持一致。
- Tauri command 不接受前端传入任意 shell 命令作为 runtime 管理接口。

## 验证策略

- Rust：`cargo check -p agentdash-local`、`cargo test -p agentdash-local`、`cargo check -p agentdash-api`、相关 repository/API 测试。
- Frontend：`pnpm --filter app-web build/check`、`pnpm --filter app-tauri check`、共享包 typecheck/test。
- Tauri：desktop dev smoke test、Tauri build 至少覆盖 Windows。
- UI：使用浏览器或 Tauri dev window 验证 Dashboard 页面、Local 设置页、MCP probe、日志 tail、runtime restart gate。

## 风险点

- `agentdash-local` 当前无限重连 loop 需要可取消化，否则 Tauri 无法安全 stop/restart。
- SessionHub、terminal、MCP call、materialization 的 active work 统计如果缺失，重启 gate 容易误杀执行。
- 前端拆包如果过早扩大，会拖慢桌面 MVP；应先移动被 desktop 实际复用的模块。
- runtime health 需要明确 owner scope；早期建议以 backend/profile 为核心，project 关联通过 accessible roots/workspace binding 派生。
