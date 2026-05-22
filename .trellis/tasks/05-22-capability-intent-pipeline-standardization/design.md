# Capability 维度管线标准化设计

## Design Direction

此前设计的 `RuntimeCapabilityEffects { tool, mcp, companion, vfs }` 仍然会把主干变成长 struct。它命名更清楚，但扩展性不够：新增一个能力维度仍要改 payload 类型、replay helper、construction 读取点和测试 fixture。

本任务目标改为 dimension registry，并采用替换式重构：

```text
CapabilityPipelineSpine
  - stores declaration records
  - stores runtime effect records
  - dispatches records by dimension key
  - owns ordering, error handling, trace

CapabilityDimensionModule
  - validates payloads for one dimension
  - converts declaration/contribution/effect payloads into typed internal structs
  - replays runtime effects
  - participates in final projection normalization
```

执行顺序是先拆模块，再替换主链路：

```text
existing parser/replay logic
  -> built-in dimension modules
  -> RuntimeCapabilityTransition envelope
  -> registry-dispatched replay
  -> remove old patch replay path
```

这保证新模型成为唯一生产路径，而不是旧 `RuntimeContextPatch` 链路旁边的兼容层。

## Core Types

目标主干类型：

```rust
pub struct RuntimeCapabilityTransition {
    pub declarations: Vec<CapabilityDeclarationRecord>,
    pub effects: Vec<RuntimeCapabilityEffectRecord>,
}

pub struct CapabilityDeclarationRecord {
    pub dimension: CapabilityDimensionKey,
    pub declaration_type: String,
    pub source: CapabilityArtifactSource,
    pub payload: serde_json::Value,
}

pub struct RuntimeCapabilityEffectRecord {
    pub dimension: CapabilityDimensionKey,
    pub effect_type: String,
    pub payload: serde_json::Value,
}

pub struct CapabilityDimensionKey(String);
```

`payload` 使用 validated JSON 的原因：

- 中心主干不需要为每个新维度修改 enum；
- plugin/extension 或 future module 可以新增维度 record；
- 内置模块仍可在边界处反序列化为强类型 payload；
- repository 仍保持 `payload_json` 容器，不引入新表结构。

## Dimension Module Interface

首版可以先定义 application 内部 trait，不要求动态第三方 Rust plugin：

```rust
pub trait CapabilityDimensionModule {
    fn key(&self) -> CapabilityDimensionKey;

    fn validate_declaration(&self, record: &CapabilityDeclarationRecord) -> Result<(), String>;
    fn validate_effect(&self, record: &RuntimeCapabilityEffectRecord) -> Result<(), String>;

    fn replay_effect(
        &self,
        state: &mut CapabilityState,
        context: &mut RuntimeCapabilityReplayContext,
        record: &RuntimeCapabilityEffectRecord,
    ) -> Result<(), String>;

    fn normalize_projection(
        &self,
        state: &mut CapabilityState,
        context: &RuntimeCapabilityProjectionContext,
    ) -> Result<(), String>;
}
```

内置 modules 首版目标：

- Tool module：decode `capability_directive` declaration 与 `set_tool_access` effect，内部复用/收纳 tool access 写入逻辑。
- MCP module：decode `set_server_set` effect，内部处理 `SessionMcpServer` payload validation 和 replay。
- Companion module：decode `set_agent_roster` effect，内部处理 companion roster replay。
- VFS module：decode `apply_vfs_overlay` / `apply_mount_operations` effect，内部收纳 overlay merge 与 mount operation application。
- Skill/guideline/runtime_surface：projection-only modules，先纳入 registry ordering 与 spec 矩阵。

这些 modules 是替换后的生产单元。旧 helper 中的维度分支逻辑迁移到 modules 后，helper 只保留为 registry dispatch façade 或直接改名为新 replay 入口。

## Current Dimension Matrix

| 维度 | Declaration Records | Contribution / Resolver | Runtime Effect Records | Projection | 首版模块状态 |
| --- | --- | --- | --- | --- | --- |
| Tool | `dimension=tool`, `declaration_type=capability_declaration` | existing `ToolContribution` | `dimension=tool`, `effect_type=set_tool_access` | `CapabilityState.tool.capabilities / enabled_clusters / tool_policy` | built-in module |
| MCP | 当前借 tool declaration 的 `mcp:<server>`；后续可 `dimension=mcp` | existing `McpCandidates` | `dimension=mcp`, `effect_type=set_server_set` | `CapabilityState.tool.mcp_servers` | built-in module |
| Companion | 暂无 declaration | existing `CompanionContribution` | `dimension=companion`, `effect_type=set_agent_roster` | `CapabilityState.companion.agents` | built-in module |
| VFS/mount | `dimension=vfs`, `declaration_type=mount_operation` | construction VFS facts | `dimension=vfs`, `effect_type=apply_vfs_overlay` / `apply_mount_operations` | final VFS / runtime surface | built-in module |
| Skill baseline | VFS files / local skill dirs | skill discovery | none | `SessionBaselineCapabilities.skills` | projection-only module |
| Guidelines | VFS/project facts | guideline discovery | none | `DiscoveredGuideline[]` | projection-only module |
| Extension runtime | installed extension assets | project extension installation repo | future extension effects | command / flag / renderer projection | projection-only module |

## Replay Contract

Replay 主干：

```text
base CapabilityState
  -> for effect in transition.effects:
       registry.module(effect.dimension).replay_effect(...)
  -> projection normalizer / module normalize_projection
  -> final CapabilityState + auxiliary projections
```

主干不出现：

```rust
state.tool = ...
state.companion = ...
state.vfs = ...
```

这些逻辑进入对应 module。

旧链路替换后的生产入口：

```rust
replay_runtime_capability_transition(base_state, transition, registry)
```

旧 `apply_runtime_context_patch` / `replay_runtime_context_patch` 命名与类型在生产代码中退出。测试 helper 如需构造 payload，也使用 `RuntimeCapabilityTransition` record builder。

## Ordering

避免“巨长链路”的关键是主干只负责两件事：

- deterministic ordering；
- dispatch by dimension key。

首版 ordering 建议：

```text
vfs -> tool -> mcp -> companion -> projection-only
```

原因：

- VFS 影响 Skill/guideline 派生；
- tool 和 MCP 都写入 `CapabilityState.tool`，MCP 依赖 tool declaration 但 effect replay 可分开；
- companion 与 tool/MCP 相对独立；
- projection-only 维度在 final facts 后派生。

该顺序不应散落在 callsite，放在 `CapabilityDimensionRegistry`。

## Extension / Plugin Fit

用户所说的模块化/插件化，不是把 pipeline 拉成更多步骤，而是让新能力能“一处注册，全链路可见”。

因此 extension 的目标接入方式应是：

```text
Extension asset / plugin manifest
  -> emits CapabilityDeclarationRecord[]
  -> optional dimension module validates declarations/effects
  -> construction reads installed extension assets
  -> registry dispatches records
  -> final projection exposes extension runtime metadata
```

Native host plugin 可以继续注册 MountProvider / Connector / extra skill dirs；runtime extension asset 应走 declaration/effect records，而不是要求主干 DTO 为每个 extension 能力加字段。

## Migration Shape

当前 payload：

```json
{
  "patch": {
    "toolDirectives": [],
    "toolIntent": {},
    "mcpIntent": {},
    "companionIntent": {},
    "vfsIntent": {}
  }
}
```

目标 payload：

```json
{
  "transition": {
    "declarations": [
      {
        "dimension": "tool",
        "declarationType": "capability_directive",
        "source": { "kind": "workflow" },
        "payload": { "add": "mcp:code_analyzer" }
      }
    ],
    "effects": [
      {
        "dimension": "mcp",
        "effectType": "set_server_set",
        "payload": { "servers": [] }
      },
      {
        "dimension": "vfs",
        "effectType": "apply_mount_operations",
        "payload": { "operations": [] }
      }
    ]
  }
}
```

具体外层字段是否仍叫 `patch` 可在 implementation 中一并改名；长期推荐 `transition`。

本任务不保留旧 JSON shape 兼容分支。项目处于预研阶段，runtime command payload 可直接切换到新 envelope。已有测试 fixture 同步更新为 new shape。

## Risks

- `serde_json::Value` 会降低编译期类型约束。缓解方式：主干只存 envelope；内置 module 必须提供 typed payload decode + validation tests。
- 过早拆 resolver 风险大。首版优先拆 runtime command replay 和 payload shape，resolver 暂时作为 contribution 生产者保留，后续再纳入 registry。
- dimension ordering 处理不好会变成隐式依赖。缓解方式：registry 集中声明顺序，并在 spec 中记录依赖。

## Validation Strategy

- serialization test：payload 是 declaration/effect record 列表，不是每维度顶层字段。
- replay test：registry dispatch 能 replay tool/MCP/companion/VFS effects。
- construction/context tests：pending VFS/MCP 后 final projection 与现有行为一致。
- extension spec check：extension runtime 后续新增能力必须产出 records 或注册 module。
