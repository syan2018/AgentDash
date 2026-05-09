# Capability 管道全链路重构

## 本次重构要解决的问题

**Agent 配置与能力管道中存在大量"概念表示不一致"问题：同一个概念在不同层用不同类型/名称/
解析方式表达，中间靠手工转换和临时字段糊弄。**

具体表现：
1. Agent 配置（base_config / config_override）是无类型 JSON blob，消费方散落 `.get("field")` 手工解析
2. `tool_clusters` 字段名称、类型、键值在前端/存储/resolver 三处互相不匹配，链路从未真正生效
3. `agent_declared_capabilities` 是一个本不该存在的中间翻译字段
4. `SessionWorkflowContext` 是一个本不该存在的中间 wrapper struct
5. `companion_slice_mode` 被错放进 capability 输入（它是 session 上下文管理概念）
6. `allowed_companions` / `mcp_preset_keys` / `display_name` 无 domain 类型表示

**本次重构不接受任何向后兼容方案、临时翻译层、或"先糊弄后面再改"。**
遇到任何同类问题（概念在某层表示不一致、存在多余的中间字段/struct、手工 JSON 解析），
一律追溯到源头修正表示，全链路贯通，一步到位。

## 重构心智模型

**每个概念只有一个权威表示，从源头到消费端一致贯通。**

看到一个字段在 A 处叫 X 类型、在 B 处叫 Y 类型、在 C 处从 JSON 手工扣——不是"需要加个转换层"，
是"源头的表示就错了"。重构 = 追溯到源头修正表示，删掉所有中间转换。

## 问题全景

### 根因：Agent.base_config 是 serde_json::Value

Agent 配置在 domain entity 中是无类型 JSON blob。`AgentConfig` struct 只覆盖部分字段，
其余字段（display_name / mcp_preset_keys / allowed_companions）散落在 `.get("field")` 手工解析中。

**手工解析散布点：**

| 字段 | 散布位置 |
|------|---------|
| tool_clusters | task/config.rs, routine/executor.rs, project_agents.rs, companion/tools.rs |
| display_name | assembler.rs, project_sessions.rs, routine/executor.rs, project_agents.rs |
| allowed_companions | assembler.rs |
| mcp_preset_keys | mcp_preset/runtime.rs |
| provider_id / model_id 等 | task/config.rs, routine/executor.rs (逐字段 .get() 反序列化) |

### tool_clusters 是死代码

前端发 ToolCluster 名称（"read", "write"），resolver 拿去跟 capability keys（"file_read", "file_write"）
比较——名称不匹配，永远 false。加上 10 个 well-known capability 中只有 `workflow_management` 有
`agent_can_grant: true`（且不在前端 checkbox 列表），整条 `tool_clusters → agent_declared_capabilities`
链路从未真正生效过。

### 三个"tool clusters"

1. `ToolCluster` enum (SPI) — 运行态工具簇（Read, Write, Execute...）
2. `AgentConfig.tool_clusters: Option<Vec<String>>` — 存储的 ToolCluster 名称字符串
3. `CapabilityState.tool.tool_clusters: BTreeSet<ToolCluster>` — 从 effective capabilities 推导

### companion_slice_mode 错放 + SessionWorkflowContext 冗余

已在先前讨论中明确：前者是 session 上下文管理概念，后者应消融进 contributions。

---

## 解法分层

### Layer 0 (Phase A): AgentPresetConfig — 消灭无类型 JSON

引入全 Option 的 `AgentPresetConfig`，统一 base_config 和 config_override，
消灭 `merge_json` 和所有手工 `.get()` 解析。

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentPresetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_mode: Option<SystemPromptMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    // ── 能力配置 ──
    /// agent 级能力指令。替代旧 tool_clusters: Option<Vec<String>>。
    /// 前端 → API → 存储 → resolver 全链路使用相同表示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_directives: Option<Vec<ToolCapabilityDirective>>,
    /// MCP Preset key 引用列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_preset_keys: Option<Vec<String>>,
    /// 允许此 agent 调用的 companion agent 名称列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_companions: Option<Vec<String>>,
}

impl AgentPresetConfig {
    /// 字段级合并：override 非 None 的字段优先。
    pub fn merge_over(&self, base: &AgentPresetConfig) -> AgentPresetConfig { ... }
}
```

**消灭的函数/代码：**
- `merge_json()` → `AgentPresetConfig::merge_over()`
- `executor_config_from_preset()` → `serde_json::from_value::<AgentPresetConfig>(preset.config)`
- `build_agent_config_from_merged()` → 同上
- 所有 `.get("display_name")` / `.get("tool_clusters")` / `.get("allowed_companions")` / `.get("mcp_preset_keys")` 手工解析
- `ProjectAgentLink::merged_config()` 返回 `AgentPresetConfig` 而非 `serde_json::Value`

**与旧 AgentConfig 的关系：**
- `AgentConfig`（SPI re-export，connector 层使用）是运行态执行配置（executor + model 参数），
  从 `AgentPresetConfig` 提取。保留但只用于 connector 接口。
- `AgentPresetConfig` 是配置存储层的权威类型（Agent.base_config / link.config_override / AgentPreset.config）。

**DB 兼容性：**
- DB 仍用 jsonb 列存储，serde 处理序列化/反序列化。
- 需要 migration 将现有 `tool_clusters: ["read", "write", ...]` 转为
  `capability_directives: [{"add": {"capability": "file_read"}}, ...]`。
  映射关系：read → file_read, write → file_write, execute → shell_execute,
  workflow → workflow, collaboration → collaboration, canvas → canvas。

### Layer 1 (Phase B): 前端全链路对齐

**agent-preset-editor.tsx:**
- `tool_clusters: ToolCluster[]` → `capability_directives: CapabilityDirective[]`
- `ToolCapabilitiesField` checkbox UI 保留，每个 checkbox 映射到
  `{ add: { capability: "file_read" } }` 而非存储 `"read"`
- `TOOL_CLUSTER_OPTIONS` → `CAPABILITY_OPTIONS`，value 改为 capability key

**types/index.ts:**
- `ToolCluster` type 重新定义或替换为 `CapabilityKey`（"file_read" | "file_write" | ...）
- 新增 `CapabilityDirective` 类型 = `{ add: { capability: string } } | { remove: { capability: string } }`

**project-agent-view.tsx:**
- 读取 `mergedConfig.capability_directives` 替代 `mergedConfig.tool_clusters`

---

### Layer 2 (Phase C): Resolver 输入侧 ContextContributions 化

> 以下为原 Phase 3 详细实施方案，完整保留。

#### 背景

Phase 0-2 已完成维度化输出（`CapabilityState { tool, companion, vfs }`）和下游冗余消除。
Phase C 的目标是将 **输入侧** 从"调用方手动填 `CapabilityResolverInput` 的 7 个字段"
重构为"各来源 ctx 统一用 `ToolCapabilityDirective` 描述能力意图，Resolver 按维度合并解析"。

#### 当前 `CapabilityResolverInput` 的 7 个字段逐一审查

```rust
pub struct CapabilityResolverInput {
    pub owner_ctx: SessionOwnerCtx,                         // ✓ 保留：归属上下文（决定 visibility 基线）
    pub agent_declared_capabilities: Option<Vec<String>>,   // ✗ 冗余：本质是 agent 声明的 directives
    pub workflow_ctx: SessionWorkflowContext,                // ✗ 冗余：本质是 workflow 产出的 directives
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,         // △ 转化：MCP 候选数据源
    pub available_presets: AvailableMcpPresets,              // △ 转化：MCP 候选数据源
    pub companion_slice_mode: Option<CompanionSliceMode>,   // ✗ 不属于此处：session 级上下文管理
    pub available_companions: Vec<CompanionAgentEntry>,      // △ 转化：Companion 候选数据源
}
```

##### `agent_declared_capabilities` 必须消灭

**本质**：agent config 中的 capability directives 就是 agent 对 Tool 维度的意图声明。

**当前实现**：Resolver 内部通过 `is_capability_visible(cap, owner_type, agent_declares_this, ...)`
做 visibility 判断。`agent_declares_this` 的语义 = `agent_can_grant && agent_declares`，
即"这个 capability 的 visibility rule 允许 agent 授予，且 agent 确实声明了"。

**统一表达**：Phase A 完成后，`AgentPresetConfig.capability_directives` 已是正确的
`Vec<ToolCapabilityDirective>` 类型，直接放入 agent contribution 的 directives 字段。
Resolver 的 visibility 判定从"agent 声明了吗？"变为"是否存在一条 agent 来源的 Add directive？"。
这与 workflow directives 完全同构——只是来源不同、优先级不同。

##### `companion_slice_mode` 必须移出

**本质**：控制 companion 子 session 从父 session **继承**能力时的裁剪策略。
这是 session 上下文管理的概念（"这个 session 是什么类型的子 session"），不是
"某个来源对能力的贡献"。

**当前用法**：只有一个调用点传入 `Some(mode)` — `compose_companion_with_workflow`。
其他所有地方都是 `None`。

**正确位置**：companion dispatch 在调用 `resolve()` 之后，用 `apply_companion_slice(state, mode)`
对结果做后处理。这个裁剪逻辑在 `resolve()` 返回之后执行，不需要混进 resolver 输入。

##### `SessionWorkflowContext` 应该消融

`SessionWorkflowContext { has_active_workflow: bool, workflow_tool_directives: Option<Vec<...>> }`
本质上就是：

- `has_active_workflow` → workflow 来源是否参与 visibility 判定
- `workflow_tool_directives` → workflow 产出的 directives

在 contribution 化之后，前者由"是否存在 workflow 来源的 contribution"判定，
后者由 contribution 中的 directives 字段承载。`SessionWorkflowContext` struct 不再需要。

#### 重构目标

将 `CapabilityResolverInput` 从 7 个散列字段收为 3 个：

```rust
pub struct CapabilityResolverInput {
    /// session 归属上下文（决定 visibility 基线 + platform MCP scope）。
    pub owner_ctx: SessionOwnerCtx,
    /// 各来源按固定优先级排列的 contributions。
    pub contributions: Vec<ContextContributions>,
    /// MCP server 候选数据源。
    pub mcp_candidates: McpCandidates,
}
```

关键改动：
- `agent_declared_capabilities` → Phase A 后 AgentPresetConfig.capability_directives 已是
  正确的 `Vec<ToolCapabilityDirective>`，直接放入 agent contribution
- `workflow_ctx` → 消融为 workflow 来源的 `ContextContributions`
- `companion_slice_mode` → 移出 resolver，由调用方在 resolve 后做后处理
- `agent_mcp_servers` + `available_presets` → 合入 `McpCandidates`
- `available_companions` → 放入 companion 来源的 `CompanionContribution`

#### 设计

##### 核心类型

```rust
/// Tool 维度的 contribution（来自单个来源）。
pub struct ToolContribution {
    /// 该来源产出的 capability directives。
    /// agent 的 directives 直接来自 AgentPresetConfig.capability_directives；
    /// workflow 产出的 directives 直接传入。
    pub directives: Vec<ToolCapabilityDirective>,
    /// 标记来源中是否存在活跃 workflow（影响 visibility 判定）。
    pub has_active_workflow: bool,
}

/// MCP server 候选数据源（独立于 contribution，按数据源分类）。
pub struct McpCandidates {
    /// project 级 MCP Preset 字典。
    pub presets: AvailableMcpPresets,
    /// agent 内联 MCP servers。
    pub agent_servers: Vec<AgentMcpServerEntry>,
}

/// Companion 维度的 contribution。
pub struct CompanionContribution {
    /// 可用 companion 候选列表。
    pub available: Vec<CompanionAgentEntry>,
}

/// 各来源对各维度的贡献汇总。
pub struct ContextContributions {
    pub tool: Option<ToolContribution>,
    pub companion: Option<CompanionContribution>,
}

/// Resolver 输入。
pub struct CapabilityResolverInput {
    pub owner_ctx: SessionOwnerCtx,
    pub contributions: Vec<ContextContributions>,
    pub mcp_candidates: McpCandidates,
}
```

##### `companion_slice_mode` 的外移

```rust
// Before（混在 resolver 输入中）：
let state = CapabilityResolver::resolve(&cap_input, platform);

// After（resolve 后做后处理）：
let mut state = CapabilityResolver::resolve(&cap_input, platform);
if let Some(mode) = companion_slice_mode {
    state = apply_companion_slice(state, mode);
}
```

`apply_companion_slice` 从 resolver.rs 的私有函数提升为 `pub fn`，
或直接挂在 `CapabilityState` 上作为方法。

##### Resolver 内部 visibility 判定改造

```rust
// Before：
fn default_visible_capabilities(
    input: &CapabilityResolverInput,
    agent_declares_set: Option<&BTreeSet<&str>>,
) -> BTreeSet<ToolCapability> {
    for &key in WELL_KNOWN_KEYS {
        let agent_declares_this = agent_declares_set.is_some_and(|set| set.contains(key));
        if is_capability_visible(cap, owner_type, agent_declares_this, has_active_workflow) {
            effective.insert(cap);
        }
    }
}

// After：
fn default_visible_capabilities(
    owner_ctx: &SessionOwnerCtx,
    merged: &MergedToolInput,
) -> BTreeSet<ToolCapability> {
    for &key in WELL_KNOWN_KEYS {
        let agent_declares_this = merged.agent_declared_keys.contains(key);
        if is_capability_visible(cap, owner_type, agent_declares_this, merged.has_active_workflow) {
            effective.insert(cap);
        }
    }
}
```

`agent_declared_keys` 从 contributions 中的 agent 来源 directives 提取
（只关心 `Add(key)` 类型的 directives）。`has_active_workflow` 从任一
contribution 的 `has_active_workflow: true` 判定。

##### MergedToolInput（Resolver 内部的合并中间态）

```rust
struct MergedToolInput {
    /// 从 agent 来源的 Add directives 提取的 key 集合（用于 visibility 判定）。
    agent_declared_keys: BTreeSet<String>,
    /// 合并后的全部 directives（按 contributions 顺序 concat）。
    directives: Vec<ToolCapabilityDirective>,
    /// 任一来源标记了 has_active_workflow。
    has_active_workflow: bool,
}
```

#### 6 个调用点的改造

| # | 文件 | 当前 | 改造后 |
|---|------|------|--------|
| 1 | `assembler.rs compose_owner_bootstrap` | agent_declared + workflow_ctx + presets + agent_mcp + companions | agent 的 directives + workflow directives + McpCandidates + CompanionContribution |
| 2 | `step_activation.rs` | StepActivationInput 传入散列字段 | StepActivationInput 改为传入 `Vec<ContextContributions>` |
| 3 | `task/context_builder.rs` | 最简版 | workflow directives → contributions |
| 4 | `project_sessions.rs` API | 预览用 | agent directives + McpCandidates |
| 5 | `story_sessions.rs` API | 预览用 | workflow directives + McpCandidates |
| 6 | 测试 | base_input() | 空 contributions + 空 candidates |

#### 实施步骤

##### Step C1: 定义类型 + 外移 companion_slice_mode（纯新增 + 小改）

1. 在 `resolver.rs` 定义 `ToolContribution` / `McpCandidates` / `CompanionContribution` / `ContextContributions` / `MergedToolInput`
2. 将 `apply_companion_slice` 提升为 `pub fn` 或 `CapabilityState` 方法
3. 从 `CapabilityResolverInput` 删除 `companion_slice_mode`
4. 唯一传 `Some(mode)` 的调用点（`compose_companion_with_workflow`）改为 resolve 后调用
5. 其他所有传 `None` 的调用点直接删除该字段

##### Step C2: Resolver 内部新增 merge + 双路径兼容

1. 新增 `merge_contributions()` 函数产出 `MergedToolInput`
2. `CapabilityResolverInput` 新增 `contributions` 字段（默认空 vec）
3. `resolve()` 内部：contributions 非空时走 merge 新路径，否则回退旧字段

##### Step C3: 逐个迁移调用点

1. `compose_owner_bootstrap` — 最复杂，优先做
2. `step_activation.rs` — 改 `StepActivationInput`
3. `task/context_builder.rs`
4. `project_sessions.rs` / `story_sessions.rs`
5. 测试

##### Step C4: 清理

1. 从 `CapabilityResolverInput` 删除旧字段
2. 删除 `SessionWorkflowContext` struct
3. 删除 Resolver 兼容路径
4. 更新 spec 文档

---

### Layer 3 (Phase D): 清理 & 命名统一

- 消除三重"tool clusters"命名混淆：
  - `CapabilityState.tool.tool_clusters` 改名为 `enabled_clusters`（运行态簇集，从 capabilities 推导）
  - `AgentPresetConfig` 中不再有 `tool_clusters` 字段
- `ToolCluster` enum 保留（它表示运行态工具簇，语义正确）
- 更新 spec 文档

---

## 验收标准

- `Agent.base_config` 反序列化为 `AgentPresetConfig`，无手工 `.get()` 解析
- `merge_json()` 删除，由 `AgentPresetConfig::merge_over()` 替代
- 前端发送 `capability_directives`，DB 存储 `capability_directives`，resolver 通过 contribution 接收
- `agent_declared_capabilities` 字段不存在
- `SessionWorkflowContext` struct 不存在
- `companion_slice_mode` 不在 `CapabilityResolverInput` 中
- `CapabilityResolverInput` 只有 `owner_ctx` + `contributions` + `mcp_candidates` 三个字段
- `cargo check` / `cargo test` 通过 + 前端构建通过

## 风险与注意事项

1. **`agent_declared_capabilities` → directives 的语义等价性**：
   当前 `is_capability_visible` 的 `agent_can_grant && agent_declares` 语义
   需要确保转化为 `Add` directive 后行为完全一致。关键点：
   - `agent_can_grant = false` 的 capability（如 `file_read`）即使 agent 声明了也不启用
   - 这意味着 agent 来源的 Add directives 需要在 resolver 内部受 `agent_can_grant` rule 约束
   - 实现方式：merge 时标记 directive 来源，或在 visibility 判定时仍使用 `agent_declared_keys` 集合

2. **Directive 顺序**：各来源 concat 的顺序影响归约结果。固定为
   Owner（baseline）→ Agent（声明）→ Project（资源）→ Workflow（覆盖）

3. **`has_active_workflow` 语义**：当前影响 `workflow_can_grant` 授予路径。
   contribution 化后由 workflow 来源的 `has_active_workflow` 标记承载。

4. **DB migration**：现存 `tool_clusters` JSON 字段必须通过 migration 一次性转为
   `capability_directives` 格式。不做 serde alias 兼容、不做双路径回退。
