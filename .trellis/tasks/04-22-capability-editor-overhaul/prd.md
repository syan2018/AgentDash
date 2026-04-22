# Capability 编辑器与 Directive 模型整体重构

## Goal

把"工作流能力声明"从"只能 Add"升级为"显式 Add/Remove 的指令序列"，并围绕这个新模型完成前端编辑器 UX 重构与平台 MCP scope 工具元数据的补齐，让 workflow 作者能自然地表达 **「本工作流希望屏蔽 agent 基线里的某个能力或某个具体工具」**，同时彻底消除"多个集合展示同一组工具"的混乱。

本任务是 2026-04 capability 系列重构的收尾：在 [`e321169`](commit-ref) 引入的 `CapabilityEntry` + `exclude_tools` 基础上，补齐屏蔽语义、静态 MCP scope 元数据、以及匹配的编辑器交互。

## What I already know

事实（来自本会话的 review 与代码审查）：

- `WorkflowContract.capabilities: Vec<CapabilityEntry>` 只承载 Add 语义；`CapabilityDirective::Add / Remove` 这个 enum 当前只在 runtime hook delta 路径使用，且 [`session_workflow_context.rs:985`](crates/agentdash-application/src/capability/session_workflow_context.rs#L985) 用 `panic!("不应出现 Remove 指令")` 硬性排除 workflow-source 的 Remove。
- Agent baseline 能力（`file_read / file_write / shell_execute / canvas / collaboration / relay_management / story_management / task_management`）在 [`tool_capability.rs:342-405`](crates/agentdash-spi/src/tool_capability.rs#L342) 中 `auto_granted=true`，也就是说**不写 workflow 也会启用** —— 想屏蔽就必须有 Remove 通路。
- `PlatformMcpScope::Relay / Story / Task / Workflow` 这 4 种 capability 的工具 descriptor 完全不在 [`platform_tool_descriptors()`](crates/agentdash-spi/src/tool_capability.rs#L230) 静态表里，[`tool_catalog`](crates/agentdash-application/src/capability/tool_catalog.rs) API 对它们返回空 vec，所以前端 CapabilitiesEditor 展开这些条目永远是"此能力无下属平台工具"。
- 前端 [`CapabilitiesEditor`](frontend/src/features/workflow/workflow-editor.tsx#L600) 只有 Add 按钮 + 勾选后才能 exclude 子工具这一条流程；对"就是想从 baseline 拿掉 shell_execute"这种典型需求没有对应入口。
- 前置清理（本会话已提交范围外已完成）：前端移除 `file_system` 别名按钮；后端 `CLUSTER_WORKFLOW_TOOLS` 与 `platform_tool_descriptors` 对齐为 `complete_lifecycle_node`；`plan.rs::conditional_flow_tools`、`script_engine::is_workflow_artifact_tool`、`stop_gate_checks_pending.rhai` 文案、`hook-script-engine.md` spec doc 全部清理掉 `report_workflow_artifact` 死引用。

对齐决策（与用户在 brainstorm 入口已对齐，详见 ADR-lite 段）：

- 数据模型：**选方案 A —— 字段整体升级为 `capability_directives: Vec<CapabilityDirective>`**。
- MCP scope 元数据：**选方案 A —— `ToolSource` 新增 `PlatformMcp { scope }` 变体**。
- 任务切分：**三条主线一次性闭环在本任务内**。

## Requirements（演进中）

### ① Workflow data model：capability_directives 升级

- 字段替换：`WorkflowContract.capabilities: Vec<CapabilityEntry>` → `capability_directives: Vec<CapabilityDirective>`。
- **老字段彻底删除**，不保留 serde 反序列化兼容；遗留数据由迁移 PR 一次性改写（DB SQL migration + builtin JSON + 所有 fixture）。
- Resolver 改造：取消 [`session_workflow_context.rs:985`](crates/agentdash-application/src/capability/session_workflow_context.rs#L985) 对 workflow-source Remove 的 `panic!` 硬保护；workflow 源的 Directive 与 runtime delta 走同一条归约路径（新 `compute_effective_capabilities` 实现 slot 规则）。
- 撤销 auto_granted 硬编码展开路径，改用新归约规则处理（NotDeclared / FullCapability / ToolWhitelist / Blocked + excluded_tools 集合）。
- `file_system` 别名 / `CAPABILITY_ALIASES` / `expand_alias` 同批次下线 —— 迁移后不再是可识别 key。

### ② CapabilitiesEditor UX 重构

- 分区重绘：顶部显示 **基线视图**（按 owner_type 动态计算的 auto_granted 能力列表），每条能力旁有「屏蔽此能力」快捷按钮和「展开查看工具」按钮；展开后每个工具前有「屏蔽此工具」。
- 次区显示 **本 workflow 追加** 的能力（非 baseline 的，如 `workflow_management`、`mcp:*`）；保留现在的勾选语义（Add 为主）。
- 屏蔽状态需要**视觉一致**：无论是"屏蔽整个能力"还是"屏蔽某几个工具"，都要有明显的 `-` 角标 + 删除线。
- 已消除的 `file_system` 并列项（本会话已改）：在 Well-known 列表只保留细粒度 key。`file_system` 不再是任何运行时识别的 key —— 在 Phase 0 迁移阶段已被自动展开为 `file_read + file_write + shell_execute`。前端不需要"未识别 key 提示 + 一键迁移按钮"这种过渡 UI。
- 工具列表展开面板：不再有「必须先勾选 Add 才能 exclude」的前置要求。任何工具行都直接提供「屏蔽此工具」按钮，点击即 emit 一条 `Remove(CapabilityPath::of_tool(cap, name))` Directive；baseline 能力（auto_granted）同理。

### ③ ToolSource 扩展 + PlatformMcp scope 工具静态注册

- `ToolSource` enum 新增 `PlatformMcp { scope: PlatformMcpScope }` 变体；`ToolDescriptor::platform_mcp(...)` 构造器。
- `platform_tool_descriptors()` 补齐四个 scope 下属工具的静态 entry：name / display_name / description / scope。具体工具名以 [`agentdash-mcp/src/servers/*.rs`](crates/agentdash-mcp/src/servers) 当前 `#[tool]` 宏注册为准（需在 research 中核对）。
- `platform_tools_for_capability(key)` 对 relay_management / story_management / task_management / workflow_management 返回正确列表 —— 前端展开面板自然可用。
- connector system prompt 组装路径：`ToolSource::PlatformMcp` 在 "### Platform Tools" 段显示（不是 MCP Tools 段），与 Cluster-based platform tool 统一呈现。
- tool_catalog API 输出保持 `ToolDescriptor[]` 形状不变，前端只需处理新 source 类型 tag。

## Acceptance Criteria

- [ ] `WorkflowContract` 序列化/反序列化测试：仅接受 `capability_directives` 字段；遇到老 `"capabilities"` key 反序列化 fail-fast 并给出明确错误信息。
- [ ] `CapabilityPath::parse` 全覆盖测试：短/长 path、mcp 前缀、非法 `::` 组合、空字符串等边界。
- [ ] Resolver 测试：workflow 声明 `Remove("shell_execute")` 能从 Project/Task/Story session 的 effective capabilities 里移除；`Remove("file_read::fs_grep")` 能让 excluded_tools 包含 `fs_grep` 而 file_read 能力仍 visible；slot 规则表中每种转移有对应断言。
- [ ] `platform_tools_for_capability("relay_management")` 返回非空列表；同样验证 story/task/workflow_management。
- [ ] 前端 CapabilitiesEditor：
  - [ ] baseline 区域能直接屏蔽 `shell_execute` 且写回 data model 为 `Remove(shell_execute)`；
  - [ ] 对 `workflow_management` 展开时看到对应工具列表（e2e 或 storybook 截图）；
  - [ ] 每个 UI 动作只产出一条 Directive，不出现"带字段的 Entry"写入。
- [ ] connector system prompt 在 "### Platform Tools" 段包含 `PlatformMcp` scope 工具，且 capability_key tag 正确。
- [ ] Phase 0 迁移完整性：
  - [ ] `builtin_workflow_admin.json` 已改写为 `capability_directives`；
  - [ ] SQL migration 脚本（up-only，遵循项目约定）可执行；
  - [ ] `cargo test --all` 通过 —— 所有 `CapabilityEntry::*` 调用已迁移；
  - [ ] 前端所有 `CapabilityEntry` 类型引用已删除。
- [ ] 所有新增/修改代码过 `cargo clippy -D warnings` 与 `tsc --noEmit`。

## Definition of Done

- 单元测试 + 关键 Resolver 场景集成测试
- `cargo test --all` 全绿（含 DB migration 往返测试）
- 前端 `tsc --noEmit` 绿、若有组件测试（vitest）也绿
- [`.trellis/spec/backend/capability/tool-capability-pipeline.md`](.trellis/spec/backend/capability/tool-capability-pipeline.md) 与 [`.trellis/spec/frontend`](.trellis/spec/frontend)（如有 CapabilitiesEditor 相关 spec）同步更新
- PR 描述包含 Phase 0 迁移清单 + 回滚指引（up-only migration，rollback 走 git revert + 手工 DDL）

## Technical Approach

### 数据模型（Hard Cutover —— 不保留兼容层）

```rust
// domain: workflow/value_objects.rs
pub struct WorkflowContract {
    // ...
    /// Workflow 对 agent baseline 的增删指令序列。
    /// 旧 `capabilities` 字段已彻底移除；遗留 JSON 在迁移阶段一次性改写。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_directives: Vec<CapabilityDirective>,
}
```

**老字段彻底删除**：
- `WorkflowContract.capabilities` 字段从 Rust struct 移除。
- `CapabilityEntry` / `CapabilityDetailedEntry` 类型从 domain 删除。
- 反序列化遇到老 `"capabilities"` key —— 报 serde 错（让上层 fail-fast 早暴露漏改）。
- 所有 fixture / builtin / DB 数据在迁移 PR 内一次性改写为 `"capability_directives"`。

迁移清单（实现 phase 0 必须穷尽）：
- [`builtin_workflow_admin.json`](crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json) —— 直接改写 JSON。
- DB 持久化层：写一次性 SQL migration 把 `workflow_definitions.contract` JSONB 中所有 `capabilities` 数组按规则展开为 `capability_directives`，老 key 删除。
- spec doc：[`tool-capability-pipeline.md`](.trellis/spec/backend/capability/tool-capability-pipeline.md) 内所有 `capabilities:` 示例改为新形式。
- 测试 fixture：[`pipeline_tests.rs`](crates/agentdash-application/src/capability/pipeline_tests.rs)、[`resolver.rs`](crates/agentdash-application/src/capability/resolver.rs) 测试中所有 `CapabilityEntry::*` 调用。
- 前端：[`types/workflow.ts`](frontend/src/types/workflow.ts) 删除 `CapabilityEntry` 类型；[`workflow-editor.tsx`](frontend/src/features/workflow/workflow-editor.tsx) `capabilityEntryKey` 等 helper 删除；store/services 改名 `capabilities` → `capability_directives`。

### 统一 Directive + CapabilityPath（已决议）

**核心洞察**：一旦 `CapabilityPath` 是一等公民，`CapabilityEntry.include_tools/exclude_tools` 就降维成多条独立的 Add/Remove Directive —— 不需要"带字段的 Entry"这种复合结构。模型整体扁平、每条指令独立自包含，前端按钮与 Directive 一一映射。

```rust
// domain/spi: capability 第一公民
pub struct CapabilityPath {
    pub capability: String,      // "file_read" / "mcp:workflow_management"
    pub tool: Option<String>,    // None → 短 path（能力级）；Some → 长 path（工具级）
}

impl CapabilityPath {
    pub fn parse(s: &str) -> Result<Self, String>;           // 解析 "cap" 或 "cap::tool"
    pub fn to_qualified_string(&self) -> String;             // 序列化回 qualified string
    pub fn is_tool_level(&self) -> bool;                     // self.tool.is_some()
    pub fn of_capability(key: impl Into<String>) -> Self;    // 短 path 构造
    pub fn of_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self;
}

// Directive 完全对称
pub enum CapabilityDirective {
    Add(CapabilityPath),
    Remove(CapabilityPath),
}
```

`CapabilityEntry` 这个类型随之**彻底删除** —— 它原本的职责（key + include/exclude tools）完全被"多条独立 Directive"覆盖。

**Directive JSON 形态**（Stage 1 已确认）：Rust `serde(rename_all = "snake_case")` 对 externally-tagged enum 产出小写 tag：

```json
[
  { "add": "file_read" },
  { "add": "file_read::fs_read" },
  { "remove": "file_read::fs_grep" },
  { "remove": "shell_execute" }
]
```

前端 TypeScript 类型必须与此 JSON 形态严格匹配（`type CapabilityDirective = { add: string } | { remove: string }`）。

### Path 语法规范

分隔符统一用 `::`（Rust 模块路径同构），与 `mcp:<server>` 的单冷号前缀不冲突。MCP server name 禁止含 `::`（`McpPreset` 验证层强制）。

| 样例                                     | 含义                        |
| ---------------------------------------- | --------------------------- |
| `file_read`                              | 短 path — 平台能力级         |
| `file_read::fs_grep`                     | 长 path — 平台 cluster 工具级 |
| `mcp:workflow_management`                | 短 path — 平台 MCP 能力级     |
| `mcp:workflow_management::upsert`        | 长 path — 平台 MCP 工具级     |
| `mcp:code_analyzer::scan`                | 长 path — 用户自定义 MCP 工具级 |

### 一次性迁移规则（Phase 0，无 compat）

老 `capabilities` entry 按下表展开为新 `capability_directives` 数组（DB migration + 所有 fixture/JSON 同步改写）：

| 老 entry                                              | 展开为                                                       |
| ----------------------------------------------------- | ------------------------------------------------------------ |
| `"file_read"` / `"file_system"` 等纯 string           | `[{"Add":"file_read"}]`                                      |
| `{"key":"file_read","exclude_tools":["fs_grep"]}`     | `[{"Add":"file_read"}, {"Remove":"file_read::fs_grep"}]`     |
| `{"key":"file_read","include_tools":["fs_read"]}`     | `[{"Add":"file_read::fs_read"}]`                             |
| `{"key":"file_read","include_tools":[a],"exclude_tools":[b]}` | 不合法组合 —— migration 层 fail-fast，人工审视后改写 |

`file_system` 别名在迁移时**自动拆解**为 `[{"Add":"file_read"},{"Add":"file_write"},{"Add":"shell_execute"}]`，别名常量在迁移后删除（`CAP_FILE_SYSTEM`、`CAPABILITY_ALIASES`、`expand_alias` 全部下线）。

### 归约规则（`compute_effective_capabilities`）

按 directive 顺序维护 per-capability 的 slot 状态：

```rust
enum SlotState {
    NotDeclared,              // 初始（baseline auto_granted 能力视作隐式 FullCapability）
    FullCapability,           // 命中过 Add(cap, None)
    ToolWhitelist(BTreeSet<String>),  // 仅命中过 Add(cap, Some(tool))
    Blocked,                  // 最后一次命中 Remove(cap, None)
}
// 工具级屏蔽独立维护
excluded_tools: BTreeMap<capability, BTreeSet<tool>>
```

规则（后来者胜）：

| 指令                              | NotDeclared        | FullCapability | ToolWhitelist{S} | Blocked        |
| --------------------------------- | ------------------ | -------------- | ---------------- | -------------- |
| `Add(cap, None)`                  | FullCapability     | -              | FullCapability   | FullCapability |
| `Add(cap, Some(t))`               | ToolWhitelist{t}   | -              | add t to S       | ToolWhitelist{t} |
| `Remove(cap, None)`               | Blocked            | Blocked        | Blocked          | -              |
| `Remove(cap, Some(t))`            | excluded_tools+=t  | excluded+=t    | S.remove(t) 或 excluded+=t | excluded+=t |

`is_capability_visible`:
- `Blocked` → false（即便 `auto_granted=true` 也不可见）
- `FullCapability` / `ToolWhitelist` → 保持原 visibility rule 判定
- `NotDeclared` → 原 visibility rule（auto_granted 仍生效）

工具可见性 = 能力可见 ∧ (FullCapability 全集减 excluded_tools) ∨ ToolWhitelist 白名单 减 excluded_tools。

### 屏蔽语义的 Runtime 合成（延伸）

[`session_workflow_context.rs::apply_workflow_directives`](crates/agentdash-application/src/capability/session_workflow_context.rs) 把 workflow-source 从 "Add-only" 放开为 "Add/Remove 均可"，删除现有 panic 保护。工作流 source 与 hook runtime source 的 Directive 走同一条 `compute_effective_capabilities` 归约路径（上文的 slot 规则），不再有双路。

### MCP Scope 工具静态元数据

```rust
// spi: tool_capability.rs
pub enum ToolSource {
    Platform { cluster: ToolCluster },
    PlatformMcp { scope: PlatformMcpScope },  // 新增
    Mcp { server_name: String },
}

pub fn platform_tool_descriptors() -> Vec<ToolDescriptor> {
    let mut out = /* 现有 cluster tools */;
    // 新增四个 scope 下的静态 entry —— 工具名以 MCP server 实际注册为准
    out.extend([
        ToolDescriptor::platform_mcp("workflows_list", "...", PlatformMcpScope::Workflow),
        // ... story/task/relay scope 下逐个列出
    ]);
    out
}
```

工具列表来源需在实现 Phase 1 中**通过研读 `crates/agentdash-mcp/src/servers/{relay,story,task,workflow}.rs`** 枚举 `#[tool]` 宏注册的 handler 函数名；一律不写业务描述时退化为函数名。

## Decision (ADR-lite)

**Context**: capability 模型经历三轮改造（string → `CapabilityEntry` → `CapabilityEntry` + `exclude_tools`），当前的 gap 是"workflow 不能表达屏蔽 baseline / 某个工具"。若继续用字段叠加（再加 `capability_blocks` 列表，或给 Add 塞更多子字段），会出现职责碎片化、前端判分支多、调试困难。

**Decision**:
- ① **数据模型统一扁平化**：`WorkflowContract.capabilities: Vec<CapabilityEntry>` 替换为 `capability_directives: Vec<CapabilityDirective>`；每条 Directive 仅有 `Add(CapabilityPath)` / `Remove(CapabilityPath)` 两种形态，**`CapabilityEntry` / `CapabilityDetailedEntry` 类型整体删除**。
- ② **路径化 addressing**：新增 `CapabilityPath { capability, tool: Option<String> }`，qualified string 形式 `cap` / `cap::tool`，分隔符 `::`。短 path 表达能力级、长 path 表达工具级，Add/Remove 均可。
- ③ `ToolSource` 新增 `PlatformMcp { scope }` 变体，避免平台 MCP / 外部 MCP 边界模糊；前端 tag 清晰可辨。
- ④ **UX** 围绕新 Directive 序列重构 CapabilitiesEditor：每个按钮发一条独立 Directive，不再维护带字段的 Entry。
- ⑤ **Hard cutover，无兼容层**：老 `capabilities` 字段 Rust 侧删除；serde 遇到老 key fail-fast；所有遗留 fixture / builtin JSON / DB 数据在 Phase 0 一次性迁移；`file_system` 别名同批次下线。

**Consequences**:
- 好：model 完全扁平；Add/Remove 对称；前端按钮 ↔ Directive 一一映射；resolver 归约路径收敛为单条 slot-based 规则；工具级屏蔽不再需要"虚拟 Add + exclude"这种绕弯；兼容代码零存量。
- 代价：Phase 0 迁移必须覆盖所有数据源（builtin JSON + SQL migration + 所有测试 fixture），漏一处就运行时报错。这是优点也是代价——借强制失败让漏网数据立即暴露。
- 风险：上线后若需要回滚到本 PR 之前的版本，DB 中已经是 `capability_directives` 字段，老代码无法读取。rollback 靠 git revert + 手工 DDL（项目现无 down migration 先例，本任务不建立新模式）。

## Out of Scope

- **已在本会话完成的小清理不在本任务范围**：`file_system` 前端按钮移除、`CLUSTER_WORKFLOW_TOOLS` 常量修正、`report_workflow_artifact` 残留清理、stop_gate preset 文案、spec doc 同步。这些会随本会话单独 commit。
- capability 更广泛的权限治理（比如 agent-config 层面的 capability 审批流、user 级别的 capability quota）不在本任务，保持仅 workflow/runtime 两个授予源。
- 新的 capability key（比如"memory"、"search"）的引入不在本任务。
- tool_catalog API 的缓存策略 / 分页 / 权限过滤 —— 保持现在的 "按 keys query → 返回全量 descriptor" 契约。
- **Add 工具级的 MVP 支持**：前端 MVP 不强求暴露「只启用一个子工具」入口（`Add(file_read::fs_read)`），数据模型允许即可；UI 暂以"能力级 Add + 工具级 Remove"为主。
- **不做反序列化兼容**：老 `capabilities` 字段不保留 serde fallback；遗留数据完全靠 Phase 0 迁移 PR 一次性改写，漏网数据在启动期 fail-fast。
- **不做双写回滚保护**：上线后回滚靠 git revert + 手工 DDL（项目 `migrations/` 下 0 个 down migration 先例，本任务遵循现有 up-only 约定，不建立新模式）。

## Technical Notes

涉及文件（预计）：

- **后端 domain**：[`crates/agentdash-domain/src/workflow/value_objects.rs`](crates/agentdash-domain/src/workflow/value_objects.rs) —— 新增 `CapabilityPath`、重写 `CapabilityDirective`、删除 `CapabilityEntry / CapabilityDetailedEntry`；`WorkflowContract.capabilities` → `capability_directives`；`compute_effective_capabilities` 重写为 slot 规则。
- **后端 SPI**：[`crates/agentdash-spi/src/tool_capability.rs`](crates/agentdash-spi/src/tool_capability.rs) —— `ToolSource::PlatformMcp` 新变体、`platform_tool_descriptors` 扩展四个 scope；删除 `CAPABILITY_ALIASES` / `expand_alias` / `CAP_FILE_SYSTEM`；`capability_entry_key` 等 helper 同步迁移。
- **后端 application resolver**：
  - [`resolver.rs`](crates/agentdash-application/src/capability/resolver.rs)（支持 PlatformMcp source 过滤 + 新 slot 归约）
  - [`session_workflow_context.rs`](crates/agentdash-application/src/capability/session_workflow_context.rs)（删除 panic 保护，改名为 `apply_capability_directives`，workflow source 与 runtime delta 同路径）
  - [`tool_catalog.rs`](crates/agentdash-application/src/capability/tool_catalog.rs)（对 MCP scope key 返回正确列表）
  - [`pipeline_tests.rs`](crates/agentdash-application/src/capability/pipeline_tests.rs)（所有 `CapabilityEntry::*` 调用改写）
- **Connector**：[`connector.rs`](crates/agentdash-executor/src/connectors/pi_agent/connector.rs) —— system prompt 分段同时识别 `ToolSource::Platform` 与 `ToolSource::PlatformMcp`，两者都归入 "### Platform Tools"。
- **MCP scope（只读研究）**：[`crates/agentdash-mcp/src/servers/{relay,story,task,workflow}.rs`](crates/agentdash-mcp/src/servers) —— 枚举 `#[tool]` 宏注册的 handler 函数名，抄到 `platform_tool_descriptors`。
- **Phase 0 迁移**：
  - [`builtin_workflow_admin.json`](crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json) —— 改写 `capabilities` → `capability_directives`。
  - [`crates/agentdash-infrastructure/src/persistence/postgres/`](crates/agentdash-infrastructure/src/persistence/postgres) —— 新增 SQL migration（up + down）批量改写 `workflow_definitions.contract` JSONB。
  - Spec：[`.trellis/spec/backend/capability/tool-capability-pipeline.md`](.trellis/spec/backend/capability/tool-capability-pipeline.md) 所有示例。
- **前端**：
  - [`frontend/src/features/workflow/workflow-editor.tsx`](frontend/src/features/workflow/workflow-editor.tsx) —— CapabilitiesEditor 重构为 baseline 视图 + workflow 追加视图。
  - [`frontend/src/types/workflow.ts`](frontend/src/types/workflow.ts) —— 删除 `CapabilityEntry` / `CapabilityDetailedEntry` / `capabilityEntryKey`；新增 `CapabilityPath` / `CapabilityDirective` / `PlatformMcp` source tag。
  - [`frontend/src/stores/workflowStore.ts`](frontend/src/stores/workflowStore.ts) + [`frontend/src/services/workflow.ts`](frontend/src/services/workflow.ts) —— 字段改名 + fetchToolCatalog 参数校验。
- **Spec**：[`.trellis/spec/backend/capability/tool-capability-pipeline.md`](.trellis/spec/backend/capability/tool-capability-pipeline.md) 全文重写示例；新增 spec 段描述 CapabilityPath 语法与 slot 归约规则。

关联活跃任务：
- `04-20-dynamic-capability-followup`（动态能力赋予收尾）—— 本任务的 Directive 升级应同步通知 runtime delta 路径保持一致（同路径合并是顺势收益）。
- `04-20-builtin-workflow-admin`（内建工作流）—— builtin workflow JSON 在 Phase 0 一并迁移。

参考：本会话中 review 的结论（5 条问题 + 优先级矩阵），以及 [`e321169`](git-ref) commit message 描述的 CapabilityEntry 现状。
