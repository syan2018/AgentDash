# Session 运行时执行路径收口

## 目标

把 session 启动后"当前在跑什么"的运行时真相收敛到一个权威容器，让 prompt 启动、热更新、follow-up、companion resume、relay 投影、连接器分派共用同一套运行时状态模型，而不是散落在 connector 私有缓存、workflow 热更新、companion 特化分支里。

## 背景（已落地的上游工作）

最近几轮 refactor 已经完成：

- **入口装配**：`SessionAssemblyBuilder` 是所有 session 启动路径的唯一入口（ACP / Story step / Routine / Workflow AgentNode / Companion），compose 函数 + `finalize_request` 产出统一的 `PromptSessionRequest`。
- **Context 合并**：`SessionContextBundle` 是所有 context fragment 的 single source，按 slot 合并去重。
- **Connector 瘦身**：`ExecutionContext` 已削到 connector-facing（保留 `assembled_system_prompt` / `assembled_tools`，删除 `mcp_servers` / `relay_mcp_server_names` / `context_bundle`）；tool 构建 / MCP 发现 / system prompt 渲染全部上提到 Application 层。
- **Connector 纯桥接**：PiAgent / Relay / Composite 不再自行组装 prompt 或发现 tool。

剩余未收口的点都集中在**"start_prompt 之后，session 正在运行什么"这一层缺权威容器**，以及由此导出的若干具体 bug。

## 代码核查确认的具体 bug

1. **`CompositeConnector` 未实现 `update_session_tools`**（[composite.rs](../../../crates/agentdash-executor/src/connectors/composite.rs)）。
   热更新走 trait 默认 no-op → live PiAgent session 的工具实际不会被替换。

2. **`SessionHub::replace_runtime_mcp_servers` 用 `HashSet::new()` 顶替 relay 名单**（[hub.rs:446-452](../../../crates/agentdash-application/src/session/hub.rs#L446-L452)）。
   根因：`SessionRuntime` 只存了 `active_mcp_servers`，没存 `active_relay_mcp_server_names`，热更新时无法还原分类。

3. **`RelayAgentConnector.session_mcp` 缓存零调用方**（[relay_connector.rs:40](../../../crates/agentdash-application/src/relay_connector.rs#L40)）。
   `set_session_mcp_servers` 全仓没有调用者 → `prompt()` 里 `cached_mcp` 恒为空 → relay payload 里 `mcp_servers` 永远是空 vec。

4. **Working-directory 归一化三处并存**：
   - `prompt_pipeline::resolve_working_dir`
   - `companion::relative_working_dir`
   - `relay_connector::relative_working_dir_string`

## 架构对齐（关键前提）

### 规则一：内嵌 connector 不自行处理 MCP

对于 **in-process / 内嵌 connector**（当前 PiAgent，以及未来同类 connector）：

- **不需要**看到原始 `McpServer` 声明。
- **不需要**关心哪些是 direct、哪些是 relay。
- **只需要**持有 Application 层预构建的 `assembled_tools: Vec<DynAgentTool>` 并直接调用。

`assembled_tools` 在 cloud 侧就已经包含：
- Runtime tools（runtime tool provider 产出）
- Direct MCP tools（`discover_mcp_tools` 产出）
- Relay MCP tools（`discover_relay_mcp_tools` 产出的 `RelayMcpToolAdapter`，执行时通过 `McpRelayProvider` 隧穿回后端）

三类 tool 都以 `DynAgentTool` 的统一 trait 形态交付给 connector；connector 不需要区分来源、不需要重做发现、不需要持有任何 MCP 状态。

**因此 `ExecutionContext` 不应当、也不需要重新加回 `mcp_servers` / `relay_mcp_server_names`。**

### 规则二：Relay connector 保留 `mcp_servers` 投影能力

对于 **`RelayAgentConnector`**（整个 executor 跑在远端/本地第三方 agent 后端）：

- cloud 侧直接把完整 `mcp_servers` 结构随 prompt payload 透传给远端 agent。
- 不区分 direct / relay，也不添加额外标注；对这些 agent 来说，MCP 建联由第三方 agent 自己处理，跟云端内嵌 agent 的工具预构建设计无关。
- `RelayAgentConnector` 只是 transport bridge，不能维护私有 per-session MCP 缓存，也不能根据 `relay_mcp_server_names` 自行分类。

换言之：relay 通道只做完整 MCP 声明透传；direct / relay 分类只服务云端 Application 层为内嵌 connector 构建 `assembled_tools`。

### 规则三：运行时真相归属 SessionRuntime，不新增对外顶层类型

当前已有三个顶层概念：
- `PromptSessionRequest`（入口装配产物）
- `SessionContextBundle`（context fragment 集合）
- `ExecutionContext`（per-turn 连接器投影）

**不再**引入平行的对外 `RuntimeExecutionEnvelope` 类型。运行时真相直接升格 `SessionRuntime`（hub.rs 内的现有 per-session 状态持有者）。

为避免重新把 active 状态散成一串弱关联字段，`SessionRuntime` 内部应持有一个小的 `ActiveSessionExecutionState` 子容器。它不是新的跨层概念，只是 `SessionRuntime` 的运行态快照字段。后续从这个 active state 产出：

- `ExecutionContext` 投影（给 connector 用，其中 `mcp_servers` 是完整原始声明；内嵌 connector 不消费，relay connector 原样透传）
- 热更新时从它读 `relay_mcp_server_names`

## 升格后的 SessionRuntime

```rust
pub(super) struct SessionRuntime {
    // 现有字段
    pub tx: broadcast::Sender<Notification>,
    pub running: bool,
    pub current_turn_id: Option<String>,
    pub cancel_requested: bool,
    pub hook_session: Option<SharedHookSessionRuntime>,

    // 当前 session 的运行态真相快照（替代分散的 active_mcp_servers 等字段）
    pub active_execution: Option<ActiveSessionExecutionState>,
}

pub(super) struct ActiveSessionExecutionState {
    pub mcp_servers: Vec<McpServer>,
    pub relay_mcp_server_names: HashSet<String>,
    pub vfs: Vfs,
    pub working_directory: PathBuf,
    pub executor_config: AgentConfig,
    pub flow_capabilities: FlowCapabilities,
    pub effective_capability_keys: BTreeSet<String>,
}
```

写入时机：`start_prompt_with_follow_up` 在完成请求解析、VFS/working_dir/capability/MCP 分类解析后，统一写入 `SessionRuntime.active_execution`。运行锁定（`running/current_turn_id`）仍可在开头先写；active execution state 应保持单一写入点，避免 MCP、relay 分类、working_dir 分散更新。

## 具体收口事项

### 1. CompositeConnector 热更新转发

实现 `CompositeConnector::update_session_tools(session_id, tools)`：
- 通过 `has_live_session(session_id)` 或已建立的 session→connector 映射找到持有 live session 的子 connector
- 转发调用到子 connector
- 若无子 connector 认领，返回明确错误而非静默 no-op

### 2. 热更新读取正确的 relay 分类

`SessionHub::replace_runtime_mcp_servers`：
- 从 `SessionRuntime.active_execution.relay_mcp_server_names` 读取分类，**删除** `HashSet::new()` 的占位
- 重算 direct/relay 分区
- 发现完工具集后，同步回写 `SessionRuntime.active_execution.mcp_servers` **和** `relay_mcp_server_names`（若热更新事件同时改动了 relay 名单）

### 3. Relay connector 清理 dead code

- **删除** `RelayAgentConnector.session_mcp` 私有缓存
- **删除** `RelayAgentConnector::set_session_mcp_servers` 方法（零调用方）
- `RelayAgentConnector::prompt` 从 `ExecutionContext.mcp_servers` 读取完整 MCP 声明，并原样序列化到 `RelayPromptRequest.mcp_servers`
- relay payload 不使用 `relay_mcp_server_names`，不区分 direct / relay
- `RelayPromptRequest.mcp_servers` 字段注释明确：这是给远端第三方 agent 的完整 MCP 声明透传，建联由远端 agent 自行处理

### 4. 工作目录路径 helper 统一

抽独立模块（候选名 `session::path_policy`），提供：
- `resolve_working_dir(mount_root: &Path, req_wd: Option<&str>) -> PathBuf`
- `to_relative_working_dir(wd: &Path, mount_root_ref: &str) -> Option<String>`

替换 `prompt_pipeline` / `companion::tools` / `relay_connector` 三处现有实现。

**本次不调整策略语义**（不在此 PR 改 fail-fast vs normalize），保留现有行为；策略变更单独任务。

### 5. Companion / Hook 自动续跑 一致性审计

- Hook 自动续跑已经走 `prompt_augmenter`（[hub.rs:857-894](../../../crates/agentdash-application/src/session/hub.rs#L857-L894)）
- Companion parent resume 当前路径需审计：是否也应该经 augmenter / 是否已通过 assembler compose
- 列出所有 `start_prompt` 内部调用点，每一个要么走 assembler compose，要么有明确理由直接构造 `PromptSessionRequest`

Companion parent resume 不要求预设必须改成走 augmenter；但本轮必须做到：
- 如果审计证明当前直接构造请求与 augmenter/assembler 路径等价，记录理由并补回归测试。
- 如果不等价（缺 owner context、active MCP/relay 分类、VFS、working_dir 或 capability），本轮修到等价，不能只留下说明。

## 验收标准

- [ ] `SessionRuntime.active_execution: Option<ActiveSessionExecutionState>` 到位，`start_prompt_with_follow_up` 有单一 active state 写入点
- [ ] `CompositeConnector::update_session_tools` 转发到持有 live session 的子 connector，并有回归测试
- [ ] `replace_runtime_mcp_servers` 热更新时从 `SessionRuntime.active_execution` 读 relay 分类；测试：热更新后 direct/relay 分类保留，live connector 工具集与 runtime 状态一致
- [ ] `RelayAgentConnector.session_mcp` / `set_session_mcp_servers` 删除
- [ ] `RelayPromptRequest.mcp_servers` 字段保留，relay connector 原样透传完整 MCP 声明，不区分 direct / relay
- [ ] `ExecutionContext` 只加回完整 `mcp_servers`，**不**加回 `relay_mcp_server_names`；doc comment 写明内嵌 connector 只调用 `assembled_tools`，relay connector 才原样透传 MCP
- [ ] 路径 helper 统一到 `session::path_policy`，三处替换完成；行为与现状一致
- [ ] Companion parent resume 路径与 hook auto-resume / assembler 路径的等价性有测试证明；如不等价则本轮修复
- [ ] 相关 spec（`.trellis/spec/backend/` 或 cross-layer）补上架构规则：
  - 规则一：内嵌 connector 不持有 MCP 状态
  - 规则二：relay MCP 是完整声明透传，建联归远端 agent
  - 规则三：运行时真相归属 SessionRuntime

## Definition of Done

- `cargo test -p agentdash-application --lib` 通过
- `cargo test -p agentdash-executor --lib` 通过
- `cargo test -p agentdash-api --lib` 通过
- `cargo test -p agentdash-local --lib` 通过
- 每个 bug 修复都有对应回归测试
- 删除的 dead code 不以"兼容层"形式保留（pre-release，不需要兼容包袱）

## 不在本次范围内

- 持久化 `SessionRuntime` active 字段作为 replay 单元
- Working-directory 策略变更（fail-fast / normalize）——单独任务
- D2a 激进方案（Hook 与 Bundle 彻底合并）——保持独立讨论
- UI 层显示 active runtime state
- Companion slice 逻辑从 `companion::tools` 的搬家（`a773fab` 已做了一部分，剩余增量独立评估）

## 实施拆分（4 个 PR）

### PR1 — SessionRuntime 升格 + 单一写入点

- 扩展 `SessionRuntime`，新增 `ActiveSessionExecutionState`
- `start_prompt_with_follow_up` 在请求解析完成后单点写入 `active_execution`
- 其他读取点（`active_mcp_servers_of` 等访问器）同步暴露新字段
- 测试：
  - 普通 prompt 启动后 SessionRuntime 所有 active 字段完整
  - 同 session 连续多轮 active 字段正确更新

### PR2 — 热更新闭环

- `CompositeConnector::update_session_tools` 转发实现
- `replace_runtime_mcp_servers` 读 `SessionRuntime.active_execution` 的 relay 名单，移除 `HashSet::new()` 占位
- 热更新后回写 SessionRuntime active 字段
- 测试：
  - Composite 转发到正确子 connector
  - Relay 分类在热更新后保留
  - SessionRuntime.active_execution.mcp_servers 与 connector 实际工具集一致

### PR3 — Relay connector 清理

- 删除 `RelayAgentConnector.session_mcp` / `set_session_mcp_servers`
- 保留 `RelayPromptRequest.mcp_servers` 字段，添加 doc comment
- relay `prompt()` 从 `ExecutionContext.mcp_servers` 原样序列化完整 MCP 声明，不使用 direct/relay 分类
- 测试：
  - Dead code 删除后编译 + 测试通过
  - relay payload 包含完整 MCP 声明
  - relay payload 不依赖 connector 私有 cache

### PR4 — 路径 helper 去重 + 内部 follow-up 审计

- 新增 `session::path_policy` 模块
- 替换三处实现
- 行为保持一致（单测覆盖 Windows 分隔符 / `.` / 空值 / 正常相对路径等）
- 列出所有生产代码里的内部 `start_prompt` / `start_prompt_with_follow_up` 调用点
- Companion parent resume 路径补等价性测试；若不等价，本 PR 内修到等价

## 技术清单：受影响文件

- `crates/agentdash-spi/src/connector.rs`（`ExecutionContext` doc comment 补充）
- `crates/agentdash-executor/src/connectors/composite.rs`
- `crates/agentdash-application/src/session/hub.rs`（SessionRuntime 定义 + 热更新）
- `crates/agentdash-application/src/session/prompt_pipeline.rs`（单一写入点 + 使用新 path helper）
- `crates/agentdash-application/src/session/path_policy.rs`（新增）
- `crates/agentdash-application/src/companion/tools.rs`（使用新 path helper；parent resume 审计注释）
- `crates/agentdash-application/src/relay_connector.rs`（清理 + 使用新 path helper）
- `crates/agentdash-application/src/backend_transport.rs`（`RelayPromptRequest.mcp_servers` doc）
- `.trellis/spec/`（三条架构规则归档）
