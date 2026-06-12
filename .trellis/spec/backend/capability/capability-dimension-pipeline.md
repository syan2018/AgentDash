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
- **modifier（运行时增量）**：`RuntimeCapabilityTransition`（declarations + effects），由 workflow / permission grant 产生，经 dimension module replay 叠加到 base 上；持久化为 `AgentFrameTransitionRecord`。

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
| Workspace module | Replace | preset `visible_workspace_module_refs` → base 投影 | dynamic visible module grant（session/runtime 暴露） | `CapabilityState.workspace_module`（`mode` 三态，经 `effective_capability_json`） | 否 | base-projection + runtime grant module |
| Skill baseline | Replace | 权限=preset `skill_asset_keys`（声明式授予）；列表=lifecycle VFS files / local skill dirs 物化 | none | `SessionBaselineCapabilities.skills`（发现物化，供上下文展示和执行侧读取） | 否 | projection-only module |
| Guidelines | — | VFS/project facts | none | `DiscoveredGuideline[]` | 否 | projection-only module |
| Extension runtime | — | installed extension assets | future extension effects | command / flag / renderer projection | 否 | projection-only module |

> **Workspace module 可见性**：声明式 allowlist 事实源是 ProjectAgent preset `visible_workspace_module_refs`，投影进 base `CapabilityState.workspace_module`（`mode=All` 未配/清空 / `mode=Allowlist` 受限），经 `effective_capability_json` 序列化还原。`workspace_module_create(kind="canvas")` materialize 新 `canvas:{mount_id}` 时可以为当前 session 追加 runtime visible module grant，使 create 后紧接着 describe/invoke/present 不被 allowlist 裁掉；这个 grant 属于 session/runtime exposure，不回写 ProjectAgent preset。
>
> **Skill 权限 vs 发现**：skill 的"授予"是 `skill_asset_keys`（声明式 Replace，种进 lifecycle mount metadata）；`CapabilityState.skill.skills`（`SkillEntry`）是 `load_skills_from_vfs` 扫 mount 的**发现物化结果**，供上下文展示和执行侧读取。`frame_builder` 的 `inherit_skills_from` carry-forward 是发现缓存（热修订不重扫 VFS），与权限原语无关。

## Canvas Workspace Module Grants

`workspace_module_create(kind="canvas")` 同时做两件事：创建或接入 Canvas 资产，并把对应 `canvas:{mount_id}` module grant 到当前 session 的 runtime visible module set。这样 Agent 在同一轮可立即 `workspace_module_describe`、`workspace_module_invoke` 或 `workspace_module_present` 该实例。

这个动态 grant 与 VFS Accumulate 维度配合使用：workspace module grant 让实例 operation/UI entry 可见，Canvas VFS exposure 让 `cvs-<mount_id>://...` 文件面和 `canvas-system` skill 可见。二者都表达当前 session 的可操作面，不改变 ProjectAgent 的长期 preset。

## Registry Ordering

`CapabilityDimensionRegistry` 集中维护内置 replay 顺序：

```text
vfs -> tool -> mcp -> companion -> projection-only
```

VFS 先 replay，使 Skill/guideline 等 projection-only 维度能从 final VFS 派生。Tool 与 MCP 都写入 `CapabilityState.tool`，但 MCP server set 独立为 MCP effect。Companion 与 tool/MCP 相对独立。

## Extension / Plugin Boundary

runtime extension asset 或 plugin 新能力接入时，产出 `CapabilityDeclarationRecord` / `RuntimeCapabilityEffectRecord`，或注册对应 dimension module。主干结构只维护 envelope、ordering、dispatch 与 projection 汇聚。
