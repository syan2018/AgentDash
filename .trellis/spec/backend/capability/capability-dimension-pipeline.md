# Capability 维度管线

Capability 维度管线把能力系统收束为稳定主干和可注册维度模块。主干负责 record envelope、ordering、dispatch 与 projection 汇聚；tool、MCP、companion、VFS/mount、Skill baseline、guidelines、runtime surface、extension runtime 等维度由各自模块表达业务语义。

## 核心术语

| 层级 | 标准名 | 含义 |
| --- | --- | --- |
| Declaration | `CapabilityDeclarationRecord` | 配置、workflow、extension asset 等来源声明的能力意图 |
| Contribution | `CapabilityContributionRecord` | 带来源身份、授权语义、候选事实的归约输入 |
| Effect | `RuntimeCapabilityEffectRecord` | runtime command 可持久化并 replay 的执行效果 |
| Projection | dimension-specific projection | connector、UI、model context 消费的闭包后输出 |
| Dimension module | `CapabilityDimensionModule` | 一个能力维度的 declaration/effect validation、typed decode、replay、projection normalize 单元 |

主干 record envelope 使用：

```text
dimension
declaration_type / effect_type
source
payload
```

`payload` 可以用 `serde_json::Value` 持久化，但 built-in module 必须在 replay 前 decode 到 module-owned typed payload，并在 module 边界完成 validation。业务 replay 内部消费强类型 payload。

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

## Built-in Dimension Matrix

| 维度 | Declaration | Runtime Effect | Projection | 模块状态 |
| --- | --- | --- | --- | --- |
| Tool | `dimension=tool`, `declaration_type=capability_directive` | `set_tool_access` | `CapabilityState.tool.capabilities / enabled_clusters / tool_policy` | built-in module |
| MCP | 当前可由 tool declaration 的 `mcp:<server>` 间接声明 | `set_server_set` | `CapabilityState.tool.mcp_servers` | built-in module |
| Companion | 当前来自 companion contribution 候选 | `set_agent_roster` | `CapabilityState.companion.agents` | built-in module |
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

runtime extension asset 或 plugin 新能力接入时，产出 `CapabilityDeclarationRecord` / `RuntimeCapabilityEffectRecord`，或注册对应 dimension module。主干结构只维护 envelope、ordering、dispatch 与 projection 汇聚；新增维度的业务 payload、validation 和 replay 由 module 拥有。
