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

多个 requested runtime command 按 store 返回顺序 fold replay 到 construction base projection。VFS/mount operation 是有序 effect；construction、context query、next-turn launch 与 pending apply event 共用同一个 transition fold replay 入口。

## Module Interface

```rust
CapabilityDimensionModule::validate_declaration(&CapabilityDeclarationRecord) -> Result<(), String>
CapabilityDimensionModule::compile_declaration(&CapabilityDeclarationRecord) -> Result<Option<CapabilityContributionRecord>, String>
CapabilityDimensionModule::validate_effect(&RuntimeCapabilityEffectRecord) -> Result<(), String>
CapabilityDimensionModule::replay_effect(&mut CapabilityState, &mut RuntimeCapabilityReplayContext, &RuntimeCapabilityEffectRecord) -> Result<(), String>
CapabilityDimensionModule::normalize_projection(&mut CapabilityState, &RuntimeCapabilityProjectionContext) -> Result<(), String>
CapabilityDimensionRegistry::register_module(module) -> Result<(), String>
CapabilityDimensionRegistry::validate_transition(&RuntimeCapabilityTransition) -> Result<(), String>
```

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

## Built-in Dimension Matrix

| 维度 | Declaration | Runtime Effect | Projection | 模块状态 |
| --- | --- | --- | --- | --- |
| Tool | `dimension=tool`, `declaration_type=capability_directive` | `set_tool_access` | `CapabilityState.tool.capabilities / enabled_clusters / tool_policy` | built-in module |
| MCP | 可由 tool declaration 的 `mcp:<server>` 间接声明 | `set_server_set` | `CapabilityState.tool.mcp_servers` | built-in module |
| Companion | companion contribution 候选 | `set_agent_roster` | `CapabilityState.companion.agents` | built-in module |
| VFS/mount | `mount_operation` | `apply_vfs_overlay` / `apply_mount_operations` | final VFS / runtime surface | built-in module |
| Skill baseline | VFS files / local skill dirs | none | `SessionBaselineCapabilities.skills` | projection-only module |
| Guidelines | VFS/project facts | none | `DiscoveredGuideline[]` | projection-only module |
| Extension runtime | installed extension assets | future extension effects | command / flag / renderer projection | projection-only module |

## Registry Ordering

`CapabilityDimensionRegistry` 集中维护内置 replay 顺序：

```text
vfs -> tool -> mcp -> companion -> projection-only
```

VFS 先 replay，使 Skill/guideline 等 projection-only 维度能从 final VFS 派生。Tool 与 MCP 都写入 `CapabilityState.tool`，但 MCP server set 独立为 MCP effect。Companion 与 tool/MCP 相对独立。

## Extension / Plugin Boundary

runtime extension asset 或 plugin 新能力接入时，产出 `CapabilityDeclarationRecord` / `RuntimeCapabilityEffectRecord`，或注册对应 dimension module。主干结构只维护 envelope、ordering、dispatch 与 projection 汇聚。
