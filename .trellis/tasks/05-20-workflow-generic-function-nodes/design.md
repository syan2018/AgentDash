# 工作流通用功能节点扩展设计

## 1. 设计判断

现有 Lifecycle 已经拥有 DAG、node type、port、artifact edge、execution log 与 lifecycle VFS。通用功能节点应接入这些既有抽象，而不是另起一套 workflow runner。

推荐设计：

- `LifecycleNodeType` 新增 `FunctionNode`。
- `LifecycleStepDefinition` 新增 `function: Option<FunctionNodeSpec>`。
- `FunctionNodeSpec` 使用 tagged enum 表达具体节点：
  - `ApiRequest(ApiRequestNodeSpec)`
  - `BashExec(BashExecNodeSpec)`
- Orchestrator 在扫描 Ready nodes 时新增 FunctionNode 分支，调用 `FunctionNodeExecutor`，成功后写 artifacts 并 complete step，失败后 fail step。

这样保持 node type 的职责清晰：

- `agent_node`：启动 Agent session。
- `phase_node`：切换当前 session 的 workflow contract / capability。
- `function_node`：平台直接执行确定性动作。

## 2. 数据模型

### 2.1 Domain 类型

建议在 `agentdash-domain/src/workflow/value_objects.rs` 增加：

```rust
pub enum LifecycleNodeType {
    AgentNode,
    PhaseNode,
    FunctionNode,
}

pub struct LifecycleStepDefinition {
    pub key: String,
    pub description: String,
    pub workflow_key: Option<String>,
    pub node_type: LifecycleNodeType,
    pub function: Option<FunctionNodeSpec>,
    pub output_ports: Vec<OutputPortDefinition>,
    pub input_ports: Vec<InputPortDefinition>,
    pub capability_config: CapabilityConfig,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionNodeSpec {
    ApiRequest(ApiRequestNodeSpec),
    BashExec(BashExecNodeSpec),
}
```

字段命名保持 snake_case，以便前后端 JSON 直接映射。

### 2.2 API 请求节点

首版 spec 建议：

```rust
pub struct ApiRequestNodeSpec {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<KeyValueTemplate>,
    pub query: Vec<KeyValueTemplate>,
    pub body: Option<RequestBodyTemplate>,
    pub timeout_ms: u64,
    pub output_mapping: ApiResponseOutputMapping,
}

pub struct ApiResponseOutputMapping {
    pub status_port: Option<String>,
    pub headers_port: Option<String>,
    pub body_port: Option<String>,
    pub json_paths: Vec<JsonPathPortMapping>,
}
```

首版可以只实现完整 body 写入，`json_paths` 在 schema 中预留但实现时作为第二步。若实现预留字段，需要校验未支持字段为空。

### 2.3 Bash 执行节点

首版 spec 建议：

```rust
pub struct BashExecNodeSpec {
    pub command: String,
    pub cwd: BashCwdSpec,
    pub env: Vec<KeyValueTemplate>,
    pub timeout_ms: u64,
    pub output_mapping: BashOutputMapping,
}

pub enum BashCwdSpec {
    WorkspaceRoot,
    RelativePath { path: String },
}

pub struct BashOutputMapping {
    pub stdout_port: Option<String>,
    pub stderr_port: Option<String>,
    pub exit_code_port: Option<String>,
    pub combined_port: Option<String>,
}
```

Bash 的实际执行应复用现有本机能力链路：

- workspace binding 解析到 backend。
- local backend 执行命令。
- 云端 orchestrator 接收结果并写入 Lifecycle artifacts。

如果当前 Relay / VFS exec 链路已能基于 workspace mount 执行，应优先复用 `RelayVfsService::exec` 或已有 `shell_exec` 能力，而不是单独新增一条命令协议。

## 3. 执行流程

### 3.1 Ready Function Node 自动执行

Orchestrator 当前在 `activate_ready_nodes` 中按 node type 分派：

- AgentNode：创建 child session。
- PhaseNode：激活 step 并应用 runtime capability。

新增：

- FunctionNode：
  1. 调用 `LifecycleRunService.activate_step` 标记 Running。
  2. 构造 `FunctionNodeExecutionContext`，包含 project_id、run_id、step_key、root session_id、lifecycle edges、input artifact 读取器、output artifact 写入器、workspace/runtime routing 信息。
  3. 调用对应 executor。
  4. 将 executor 结果按 output mapping 写入 `lifecycle://artifacts/{port_key}` 底层 inline file。
  5. 追加 execution log。
  6. 成功则 `complete_step`，失败则 `fail_step`。
  7. 再次触发 `activate_ready_nodes`，让连续 function nodes 可以串行推进。

### 3.2 Artifact 写入

优先复用 `LifecycleJourneyProjection::write_port_output` 的 inline file 机制。Function node executor 不直接操作数据库 repository；由应用层提供 `WorkflowArtifactWriter` port。

写入策略：

- 每个 output mapping 只允许写入当前 step 声明的 output port。
- 未声明 mapping 的输出只进入 execution_log detail，不作为 artifact edge 的数据源。
- 超大输出按限制截断，并在 metadata 中标记 `truncated: true`。

### 3.3 失败语义

API 请求失败分两层：

- transport / timeout / invalid config：节点 Failed。
- HTTP 非 2xx：默认节点 Failed，但 spec 可预留 `success_statuses` 以支持 201 / 204 等显式成功范围。

Bash 执行失败：

- 命令无法启动 / timeout：节点 Failed。
- exit_code 非 0：默认节点 Failed，并仍可写出 stdout / stderr / exit_code artifacts 方便诊断。

## 4. 云端 / 本机边界

### API 请求

推荐首版默认云端执行 API 请求，因为云端 orchestrator 已持有 lifecycle run 状态，可以同步写 artifact 和推进节点。

保留执行位置字段：

```rust
pub enum FunctionExecutionPlacement {
    Cloud,
    LocalBackend,
}
```

首版可以只开放 `cloud`，但 Bash 必须使用 `local_backend`。如果用户明确要求 API 请求访问本机网络或 workspace secret，再实现 local placement。

### Bash 执行

Bash 必须本机执行：

- 云端只负责鉴权、run 状态、参数解析、路由和结果落库。
- 本机 runtime 负责进程执行、cwd 限制、timeout、stdout/stderr 收集。
- workspace root 必须来自 `Task.workspace_id -> WorkspaceResolution -> WorkspaceBinding.backend_id` 的路由模型。

## 5. 前端设计

### 5.1 类型与 Store

`packages/app-web/src/types/workflow.ts`：

- `LifecycleNodeType = "agent_node" | "phase_node" | "function_node"`。
- `LifecycleStepDefinition.function?: FunctionNodeSpec | null`。
- 新增 `ApiRequestNodeSpec` / `BashExecNodeSpec` TS 类型。

`workflowStore.ts`：

- 新增 step 时默认仍为 `agent_node`。
- 切换到 `function_node` 时初始化 `function` spec。
- 保存 function node 时不强制创建 workflow draft；`workflow_key` 应为 `null`。
- 删除 / 重命名 step 的 edge 同步逻辑保持不变。

### 5.2 Inspector

Step Inspector 增加节点类型选择后的条件面板：

- Agent / Phase：显示现有 workflow contract detail。
- Function：显示 Function 配置，不显示 Injection / Capability / Hooks 面板。

Function 配置面板按 kind 切换：

- API Request：method、url、headers、query、body、timeout、output mapping。
- Bash Exec：command、cwd、env、timeout、output mapping。

Ports 仍作为 DAG 真相源展示，output mapping 只能选择当前 step 的 output ports。

### 5.3 DAG 与运行视图

`dag-node.tsx` 增加 Function 标签和图标/颜色。`lifecycle-session-view.tsx` 显示 function node 的 summary、失败原因、输出 port 状态。

## 6. 校验

Domain 校验：

- `function_node` 必须有 `function`。
- 非 `function_node` 不应携带 `function`。
- `function_node` 不需要 `workflow_key`。
- entry step 仍必须是 `agent_node`，除非未来明确支持无 Agent 的纯 function lifecycle。

Catalog 校验：

- function output mapping 引用的 port 必须存在于 step.output_ports。
- Bash cwd 不能逃逸 workspace root。
- timeout 必须在平台允许范围内。
- header/env key 不能为空。
- API url 必须是 http/https。

前端校验：

- 与后端保持同构，但以后端为最终权威。

## 7. 迁移

`lifecycle_definitions.steps` 是 JSONB 序列化结构。新增 `function` 字段和 `function_node` enum value 不一定需要数据库结构迁移。

需要确认：

- 如果没有 DB enum 或 CHECK constraint 限制 node type，则不加迁移。
- 如果前端/后端 seed template 需要内置 function node 示例，更新 builtin JSON。
- 若 execution log event kind 增加 `function_started` / `function_completed`，因为 execution_log 也是 JSONB，通常不需要结构迁移，但需要 DTO/TS 类型同步。

## 8. 风险与取舍

- Function node 自动推进会让 orchestrator 具备递归推进能力，需要防止连续 function nodes 长链阻塞请求线程。实现时应设置每轮推进上限，或用异步任务队列承载。
- Bash 节点涉及本机命令执行，必须严格复用 workspace routing 和 cwd 约束。
- API 请求节点未来会牵涉 secret 管理，本轮只设计 secret reference，不把明文 secret 存进 lifecycle definition。
- Function node 不绑定 workflow contract，会让前端当前“每 step 一个 workflow draft”的保存模型需要分支处理。

## 9. 建议的 MVP

MVP 范围建议：

- 新增 `function_node` 与 typed function spec。
- 实现 API Request 云端执行。
- 实现 Bash Exec 本机执行。
- 输出只支持将完整结果字段映射到 port：status/body/stdout/stderr/exit_code/combined。
- 不实现 JSONPath、重试、条件分支、secret 管理 UI。
- 前端提供可编辑表单和运行结果展示。

