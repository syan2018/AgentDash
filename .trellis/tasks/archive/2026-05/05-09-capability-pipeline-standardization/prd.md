# Capability 管道标准化 — 维度化收口与链路简化

## 背景

远端 `05-08-workflow-tool-surface-filtering` 和 `05-08-runtime-capability-state-consolidation`
两轮重构已完成：

- `CapabilityState` 作为运行态唯一能力容器（`capabilities` / `tool_clusters` /
  `tool_policy` / `mcp_servers` / `vfs`）
- `CapabilityResolverOutput` 简化为 `{ pub state: CapabilityState }`
- `tool_policy` 取代旧 `excluded_tools`，统一工具级过滤
- `CapabilityStateDelta` 统一 diff/通知
- Tool schema 热更收敛、Hook lifecycle 收敛

但 **CapabilityState 只是输出侧名义统一**——实际数据流仍有三类冗余：

| 冗余类型 | 现状 |
|----------|------|
| 字段并行 | `SessionProfile` 同时持有 `vfs` + `mcp_servers` + `capability_state`（后者内部也含 mcp_servers/vfs） |
| Turn 层 | `TurnExecution.session_frame.{mcp_servers, vfs}` 与 `TurnExecution.capability_state.{mcp_servers, vfs}` 并行 |
| Pipeline 补丁 | `prompt_pipeline.rs` 手动 `base_capability_state.mcp_servers = ...` 修补 resolver 产出 |
| tool_builder 拼合 | `get_current_capability_state()` 手动从 session_frame 拷贝 mcp/vfs 到 state |
| Companion 独立链路 | companion 仍走 HookProvider markdown 注入 → baseline_capabilities 解析 → context_bundle fragment，未纳入 CapabilityState |

**根因**：Resolver 产出的 `CapabilityState` 只含平台 MCP（从 capability 派生），不含
session-level VFS 和 MCP fallback。Pipeline 需要额外补丁才能得到完整状态。Companion 完全
独立于 capability 链路。

## 目标

1. **`CapabilityState` 维度化**：内部按 Tool / Companion / VFS 三个子 struct 组织
2. **Pipeline 一次性组装**：resolver 产出 partial state → pipeline 补全为 complete state → 之后所有下游只读取
3. **消除所有并行字段**：SessionProfile / TurnExecution / PreparedSessionInputs 只持有一个 `CapabilityState`
4. **Companion 纳入 capability 管道**：不再走 HookProvider markdown 注入
5. **链路长度收窄**：当前 6 层数据传递 → 目标 3 层

## 非目标

- 不做旧字段兼容（预研期，直接硬切）
- 不重做 MCP host 全局 tools/list 协议
- 不扩展前端 capability editor

## 目标数据模型

### CapabilityState（唯一运行态容器 = Resolver 最终产出）

```rust
/// 工具 + MCP 维度。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDimension {
    /// 最终生效的 capability key 全集。
    pub capabilities: BTreeSet<ToolCapability>,
    /// capability 展开后的本地工具簇。
    pub tool_clusters: BTreeSet<ToolCluster>,
    /// 运行态唯一工具级过滤表。
    pub tool_policy: BTreeMap<String, ToolCapabilityFilter>,
    /// 平台 + 自定义 MCP server 完整列表。
    pub mcp_servers: Vec<SessionMcpServer>,
}

/// Companion 维度。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionDimension {
    /// 当前 session 可调用的 companion agent 列表。
    pub agents: Vec<CompanionAgentEntry>,
}

/// VFS 维度。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfsDimension {
    /// 运行态文件/上下文访问状态。
    pub active: Option<Vfs>,
}

/// 解析后的能力运行态 — 唯一状态容器。
///
/// = Resolver 最终产出 = 运行时唯一真相 = delta 基准。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityState {
    pub tool: ToolDimension,
    pub companion: CompanionDimension,
    pub vfs: VfsDimension,
}

/// Resolver 输出 = CapabilityState。
pub type CapabilityResolverOutput = CapabilityState;
```

### 链路对比

#### 当前链路（6 层 + 手动补丁）

```
CapabilityResolver::resolve()
  → CapabilityResolverOutput { state: CapabilityState }  (partial: 无 VFS、仅平台 MCP)
    → SessionAssemblyBuilder.with_capability_state(state)
      → PreparedSessionInputs { mcp_servers, vfs, capability_state }  (三字段并行)
        → finalize_request() → PromptSessionRequest { mcp_servers, vfs, capability_state }  (仍并行)
          → prompt_pipeline: base_capability_state.mcp_servers = ... ; .vfs = ...  (手动补丁)
            → SessionProfile { vfs, mcp_servers, capability_state }  (缓存仍冗余)
              → TurnExecution { session_frame.{mcp_servers,vfs}, capability_state }  (又冗余)
                → tool_builder: state.mcp_servers = session_frame.mcp_servers  (再次手动拼合)
```

**问题**：数据流经 6 层，每层都有并行字段，至少 3 处手动补丁。

#### 目标链路（3 层，零补丁）

```
CapabilityResolver::resolve()
  → CapabilityState { tool, companion, vfs }  (partial: vfs 可能为空)
    → prompt_pipeline: 一次性补全 state.vfs + merge session-level MCP
      → Complete CapabilityState  (唯一真相，此后只读)
        ├── SessionProfile = CapabilityState  (直接持有)
        ├── TurnExecution.capability_state  (直接持有)
        ├── ExecutionSessionFrame.{mcp_servers, vfs}  (从 state 派生的协议字段)
        └── tool_builder / delta / notification  (直接读 state)
```

**收益**：
- 链路从 6 层收到 3 层
- 消除所有并行字段和手动补丁
- 新增维度只需在 CapabilityState 加一个子 struct
- delta 计算天然 per-dimension

### ExecutionSessionFrame 的定位

`ExecutionSessionFrame` 是 SPI connector 层的**协议 struct**——relay connector
需要将 `mcp_servers` / `vfs` 原样下发给远端 agent。它保留 `mcp_servers` / `vfs`
字段，但这些字段在 pipeline 组装时从 `CapabilityState` 派生：

```rust
let session_frame = ExecutionSessionFrame {
    mcp_servers: state.tool.mcp_servers.clone(),
    vfs: state.vfs.active.clone(),
    // ...其他字段
};
```

不再存在"session_frame 是 mcp/vfs 的源"这种情况。

### SessionProfile 简化

```rust
// Before (冗余)
pub struct SessionProfile {
    pub vfs: Vfs,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub capability_state: CapabilityState,
}

// After (唯一真相)
pub struct SessionProfile {
    pub capability_state: CapabilityState,
}
```

### PreparedSessionInputs 简化

```rust
// Before (三字段并行)
pub struct PreparedSessionInputs {
    pub mcp_servers: Vec<SessionMcpServer>,
    pub vfs: Option<Vfs>,
    pub capability_state: Option<CapabilityState>,
    // ...其他字段
}

// After (只持有 state)
pub struct PreparedSessionInputs {
    pub capability_state: Option<CapabilityState>,
    // ...其他字段（无 mcp_servers / vfs）
}
```

## Companion 收束

### 当前链路

```
HookProvider.build_companion_agents_injection()
  → 查询 agent_links → 查 agent_repo → 格式化 markdown injection
    → HookSnapshot.injections[slot="companion_agents"]
      → baseline_capabilities.rs: parse_companion_agents_from_markdown()
        → SessionBaselineCapabilities.companion_agents
          → system_prompt_assembler 渲染
            → context_bundle fragment
```

**问题**：6 步链路、markdown 序列化反序列化、依赖 hook snapshot 加载时机。

### 目标链路

```
assembler compose_owner_bootstrap():
  查询 agent_links → 过滤 allowed_companions
    → CapabilityResolverInput.available_companions
      → CapabilityResolver::resolve() → state.companion.agents
        → pipeline 直读 state.companion → context_bundle fragment 或 system prompt
```

**收益**：
- 4 步链路（比原来少 2 步）
- 无 markdown 序列化/反序列化
- 不依赖 hook snapshot
- Companion 的可用性与 `CAP_COLLABORATION` capability 联动

## 实施计划

### Phase 0: CapabilityState 维度化

- 在 `agentdash-spi/src/connector/mod.rs` 引入 `ToolDimension` / `CompanionDimension` / `VfsDimension`
- 重构 `CapabilityState` 字段到子 struct
- 全局更新所有 `.capabilities` / `.tool_clusters` 等直接访问为 `.tool.capabilities` / `.tool.tool_clusters`
- 更新 `CapabilityStateDelta` 计算逻辑
- 保证编译通过 + 已有测试通过

### Phase 1: Companion 收束

- Resolver 输入增加 `available_companions: Vec<CompanionAgentEntry>` + `allowed_companions_filter: Option<Vec<String>>`
- Resolver 产出 `state.companion.agents`（过滤逻辑 + CAP_COLLABORATION 判定）
- assembler compose_owner_bootstrap 查询 agent_links 注入 companion candidates
- 删除 `HookProvider.build_companion_agents_injection` + `resolve_caller_allowed_companions`
- 删除 `baseline_capabilities.rs` 中的 `extract_companion_agents` / `parse_companion_agents_from_markdown`
- companion 列表改从 `state.companion.agents` 直接输出到 context_bundle fragment

### Phase 2: 消除下游冗余

- `SessionProfile` 只保留 `capability_state: CapabilityState`
- Pipeline 一次性补全 `state.vfs` + merge session-level MCP 到 `state.tool.mcp_servers`
- 消除 `PreparedSessionInputs` / `PromptSessionRequest` 的并行 `mcp_servers` / `vfs` 字段
- `tool_builder.rs` 直读 `capability_state` 不再手动拼合
- `ExecutionSessionFrame.{mcp_servers, vfs}` 改为从 state 派生
- 统一 `CompanionSliceMode`（删除 tools.rs 副本和 map_slice_mode 桥接）

### Phase 3: 输入侧 ContextContributions 化

- 定义 `ContextContributions` / `ToolContribution` / `CompanionContribution` / `VfsContribution`
- 各 compose 入口重构为 ctx.contribute() → resolve → assemble 模式
- Directive 归约顺序固定：OwnerCtx → AgentCtx → ProjectCtx → WorkflowCtx

### Phase 4: 收尾清理

- `type CapabilityResolverOutput = CapabilityState`（删除 wrapper struct）
- 清理残余 FlowCapabilities 引用
- 更新 `.trellis/spec/` 文档

## 验收标准

- 非测试代码中不再出现 `SessionProfile.vfs` / `SessionProfile.mcp_servers` 独立字段
- `prompt_pipeline.rs` 中无手动 `state.mcp_servers = ...` / `state.vfs = ...` 补丁
- `tool_builder.rs` 中无 `session_frame.mcp_servers/vfs` 拷贝到 state 的逻辑
- Companion 不再通过 HookProvider markdown 注入
- `cargo test` 通过 + `cargo clippy` clean
- 前端类型检查通过（无 API 契约变更）
- Workflow Admin Plan 阶段仍正确屏蔽 upsert 工具

## 风险点

- `CapabilityState` 广泛序列化在 session meta / pending transition 中，字段重组需同步
  序列化契约；预研期不做兼容，直接硬切
- Relay connector 消费 `ExecutionSessionFrame.{mcp_servers, vfs}`，派生逻辑需验证
- Companion 从 hook 注入迁移到 resolver 后，hook snapshot refresh 不再更新 companion —
  需确保无副作用

## 相关规范

- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/bundle-main-datasource.md`
