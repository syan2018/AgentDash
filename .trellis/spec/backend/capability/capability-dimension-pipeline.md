# Capability Dimension Pipeline

Capability 维度管线把能力系统收束为稳定主干和可注册维度模块。主干负责 record envelope、ordering、dispatch 与 projection 汇聚；各维度模块拥有自己的业务 payload、validation 和 replay。

模块不变量见 [Capability Architecture](./architecture.md)。

## Core Terms

| 层级 | 标准名 | 含义 |
| --- | --- | --- |
| Declaration | `CapabilityDeclarationRecord` | 配置、workflow、extension asset 等来源声明的能力意图 |
| Contribution | `CapabilityContributionRecord` | 带来源身份、授权语义、候选事实的归约输入 |
| Effect | `RuntimeCapabilityEffectRecord` | runtime command 可持久化并 replay 的执行效果 |
| Projection | dimension-specific projection | connector、UI、model context 消费的闭包后输出 |
| Dimension module | `CapabilityDimensionModule` | 一个能力维度的 declaration/effect validation、typed decode、replay、projection normalize 单元 |
| Accumulation policy | `AccumulationPolicy` | 维度声明其更新积累规则：`Replace` / `Accumulate` / `Ephemeral` |
| Artifact source | `CapabilityArtifactSource` | 来源标识：`preset` / `workflow` / `permission_grant` |

Record envelope:

```text
dimension
declaration_type / effect_type
source
payload
```

`payload` 可以用 `serde_json::Value` 持久化，但 built-in module 必须在 replay 前 decode 到 module-owned typed payload，并在 module 边界完成 validation。

## Runtime Transition Contract

runtime command payload 保存 `PendingCapabilityStateTransition`，其中能力变化字段为：

```rust
RuntimeCapabilityTransition {
    declarations: Vec<CapabilityDeclarationRecord>,
    effects: Vec<RuntimeCapabilityEffectRecord>,
}
```

payload 不保存完整 `CapabilityState`、`ToolDimension`、`CompanionDimension`、runtime surface、Skill baseline 或 guidelines projection。Skill baseline、guidelines 与 runtime surface 从 replay 后的 effective VFS / MCP / capability facts 派生。

多个 requested runtime command 按 store 返回顺序 fold replay 到 frame runtime surface。VFS/mount operation 是有序 effect；frame construction、context query、next-turn launch 与 pending apply event 共用同一个 transition fold replay 入口。

## Module Interface

```rust
CapabilityDimensionModule::key(&self) -> &'static str
CapabilityDimensionModule::policy(&self) -> AccumulationPolicy
CapabilityDimensionModule::validate_declaration(&CapabilityDeclarationRecord) -> Result<(), String>
CapabilityDimensionModule::compile_declaration(&CapabilityDeclarationRecord) -> Result<Option<CapabilityContributionRecord>, String>
CapabilityDimensionModule::validate_effect(&RuntimeCapabilityEffectRecord) -> Result<(), String>
CapabilityDimensionModule::replay_effect(&mut CapabilityState, &mut RuntimeCapabilityReplayContext, &RuntimeCapabilityEffectRecord) -> Result<(), String>
CapabilityDimensionModule::normalize_projection(&mut CapabilityState, &RuntimeCapabilityProjectionContext) -> Result<(), String>
CapabilityDimensionRegistry::register_module(module) -> Result<(), String>
CapabilityDimensionRegistry::validate_transition(&RuntimeCapabilityTransition) -> Result<(), String>
```

`policy()` 无默认实现，强制每个维度显式声明积累规则。

## Contract

- `RuntimeCapabilityTransition` 只由 `declarations` 与 `effects` 两组 records 组成。
- 生产代码通过 dimension module builder 产出 records；不使用聚合所有维度字段的 transition input struct。
- Tool declaration 使用 `dimension=tool / declaration_type=capability_directive`。
- VFS mount declaration 使用 `dimension=vfs / declaration_type=mount_operation`。
- Tool / MCP / Companion / VFS effect payload 在对应 module 边界 decode 为强类型 payload。
- registry 先 validate declarations/effects，再按注册顺序 replay effects。

## Validation And Errors

| 条件 | 语义 |
| --- | --- |
| declaration dimension 未注册 | transition validation error |
| effect dimension 未注册 | transition validation error |
| declaration type 不属于该 module | module validation error |
| effect type 不属于该 module | module validation error |
| payload 无法 decode 到 module-owned typed payload | module validation error |
| 重复注册同一个 dimension key | registry registration error |

## Base ⊕ Modifier 分层

有效能力面 = `resolve(base, modifiers)`，两层逻辑分离（非物理拆字段）：

- **base（声明式真值）**：ProjectAgent preset 声明投影进 base `CapabilityState`，materialized 进 `AgentFrame.effective_capability_json`。每个 revision 由当前 config 重新投影——声明式维度清空即回默认，不存在"继承上一版声明"。
- **modifier（运行时增量）**：`RuntimeCapabilityTransition`（declarations + effects），由 workflow 或 AgentRun Grant 系统中的工具集拓展请求产生，经 dimension module replay 叠加到 base 上；持久化为 `AgentFrameTransitionRecord`。工具内部准入类 Grant 不进入 capability modifier，而是保留为 AgentRun admission projection。

## Accumulation Policy

每个维度声明一个 `AccumulationPolicy`，统一表达更新如何积累，替代各处手写 merge：

| Policy | 语义 |
| --- | --- |
| `Replace` | 声明式整体替换；清空 = 回默认/全集；不跨 revision 累积 |
| `Accumulate` | modifier 跨 revision 叠加，直到显式撤销 |
| `Ephemeral` | 仅当前 revision 有效，即用即弃（预留，当前无） |

## Built-in Dimension Matrix

| 维度 | Policy | Declaration | Runtime Effect | Projection | 进 `intersect()` | 模块状态 |
| --- | --- | --- | --- | --- | --- | --- |
| Tool | Replace | `dimension=tool`, `declaration_type=capability_directive` | `set_tool_access` | `CapabilityState.tool.capabilities / enabled_clusters / tool_policy` | 是（集合交） | built-in module |
| MCP | Replace | 可由 tool declaration 的 `mcp:<server>` 间接声明 | `set_server_set` | `CapabilityState.tool.mcp_servers` | 否 | built-in module |
| Companion | Replace | companion contribution 候选 | `set_agent_roster`（定义未用，resolver 单源） | `CapabilityState.companion.agents` | 否 | built-in module |
| VFS/mount | Accumulate | `mount_operation` | `apply_vfs_overlay` / `apply_mount_operations` | final VFS / runtime surface（含 canvas mount 累积） | 否 | built-in module |
| Workspace module | Replace | preset `visible_workspace_module_refs` → base 投影 | AgentRun toolset expansion / runtime exposure revision | `CapabilityState.workspace_module`（`mode` 三态，经 `effective_capability_json`） | 否 | base-projection + AgentRun exposure projection |
| Skill baseline | Replace | 权限=preset `skill_asset_keys`（声明式授予）；列表=lifecycle VFS files / local skill dirs 物化 | none | `SessionBaselineCapabilities.skills`（发现物化，供上下文展示和执行侧读取） | 否 | projection-only module |
| Guidelines | — | VFS/project facts | none | `DiscoveredGuideline[]` | 否 | projection-only module |
| Memory discovery | — | Host Integration providers + final VFS facts | none | `MemoryDiscoveryOutput`（source inventory + bounded index） | 否 | projection-only module |
| Extension runtime | — | installed extension assets | future extension effects | command / flag / renderer projection | 否 | projection-only module |

> **Workspace module 可见性**：声明式 allowlist 事实源是 ProjectAgent preset `visible_workspace_module_refs`，投影进 base `CapabilityState.workspace_module`（`mode=All` 未配/清空 / `mode=Allowlist` 受限），经 `effective_capability_json` 序列化还原。`workspace_module_operate(operation="canvas.*")` materialize 新 `canvas:{canvas_mount_id}` 时通过 AgentRun exposure revision 追加 runtime visible module ref，使 operate 后紧接着 describe/invoke/present 不被 allowlist 裁掉；这个 runtime exposure 属于 AgentRun 当前能力面，不回写 ProjectAgent preset。
>
> **Skill 权限 vs 发现**：skill 的"授予"是 `skill_asset_keys`（声明式 Replace，种进 lifecycle mount metadata）；`CapabilityState.skill.skills`（`SkillEntry`）是 `load_skills_from_vfs` 扫 mount 的**发现物化结果**，供上下文展示和执行侧读取。`frame_builder` 的 `inherit_skills_from` carry-forward 是发现缓存（热修订不重扫 VFS），与权限原语无关。

> **Memory discovery 事实源**：memory 没有 capability declaration/effect，也不进入 `CapabilityState`。`MemoryDiscoveryOutput` 从 final VFS 与 Host Integration `MemoryDiscoveryProvider` 派生，并作为 launch plan 的 connector context 投影进入 `memory_context` frame。这样 Agent 能看到可用 memory source 和 bounded index，但读写仍必须通过原有 VFS mount capability 完成。

> **Companion roster 事实源**：可派发 companion agent 列表归属 `CapabilityState.companion.agents`。CAP snapshot / delta ContextFrame 从该投影生成 `companion_agent_roster_delta` section，供模型上下文、前端 timeline 和调试视图消费。这样 companion 工具可用性（`tool.capabilities` 中的 `collaboration`）与可派发对象列表（`companion.agents`）在同一能力状态闭包下观察，runtime transition、context query 和前端展示使用同一份投影。

## Canvas Workspace Module Runtime Exposure

`workspace_module_operate(operation="canvas.create" | "canvas.attach" | "canvas.copy")` 同时做两件事：执行 Canvas 平台层 materialize/权限行为，并把对应 `canvas:{canvas_mount_id}` runtime visible module ref 写入 AgentRun 当前 frame revision。这样 Agent 在同一轮可立即 `workspace_module_describe`、`workspace_module_invoke` 或 `workspace_module_present` 该实例。

这个 runtime exposure 与 VFS Accumulate 维度配合使用：workspace module ref 让实例 operation/UI entry 可见，Canvas VFS exposure 让 `{canvas_mount_id}://...` 文件面可见；`canvas-system` 作为 lifecycle-projected SkillAsset 进入同一 AgentRun skill baseline。它们都表达当前 AgentRun 的可操作面，不改变 ProjectAgent 的长期 preset。

Canvas runtime observation 与 interaction snapshot 不是 capability transition。它们由 AgentRun→Canvas 引用上的 runtime state repository 保存，Agent 通过 `canvas.inspect_render_state` / `canvas.get_interaction_state` operation 查询 latest facts；查询本身不追加 mailbox、不修改 frame revision，也不把状态自动写入模型历史。只有 Canvas source 通过 `window.agentdash.agent.submit(...)` 发起显式用户动作时，后端才把请求转换为 canonical `UserInput` 并进入 AgentRun mailbox。

## Registry Ordering

`CapabilityDimensionRegistry` 集中维护内置 replay 顺序：

```text
vfs -> tool -> mcp -> companion -> projection-only
```

VFS 先 replay，使 Skill/guideline 等 projection-only 维度能从 final VFS 派生。Tool 与 MCP 都写入 `CapabilityState.tool`，但 MCP server set 独立为 MCP effect。Companion 与 tool/MCP 相对独立。

## Extension / Plugin Boundary

runtime extension asset 或 plugin 新能力接入时，产出 `CapabilityDeclarationRecord` / `RuntimeCapabilityEffectRecord`，或注册对应 dimension module。主干结构只维护 envelope、ordering、dispatch 与 projection 汇聚。
