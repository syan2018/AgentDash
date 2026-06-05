# Capability Architecture

## Role

Capability 子系统统一描述 session 能力声明、runtime transition、工具/MCP/VFS/companion/skill/guideline/extension 等维度的投影闭包。它防止能力知识散落在各 session 创建路径和 runtime update 分支中。

## Invariants

- 所有 session 工具集由 `CapabilityResolver` 或 capability dimension pipeline 统一计算，不在 session 创建路径硬编码。
- `ToolCapability` 是开放 string key，不是封闭枚举。
- runtime transition payload 保存 declaration/effect records，不保存完整 `CapabilityState`、runtime surface、Skill baseline 或 guidelines projection。
- built-in dimension module 必须在 replay 前 decode 并 validate 自己的 typed payload。
- 新能力维度通过 dimension module 注册进入 validation、replay 和 projection normalize，不扩展中心化 transition input struct。
- 所有工具暴露入口必须消费 `CapabilityState.tool_policy` / capability-aware 判定。

## Current Baseline

内置维度：

| Dimension | 作用 |
| --- | --- |
| Tool | capability directive 与工具级策略 |
| MCP | MCP server set |
| Companion | companion roster |
| VFS/mount | VFS overlay 与 mount operations |
| Skill baseline | 从 VFS / local skill dirs 派生 |
| Guidelines | 从 VFS / project facts 派生 |
| Extension runtime | installed extension declaration projection |

Host Integration API 当前按 Stable / Experimental / Internal 分层，企业仓只能追加集成，不能维护独立宿主装配逻辑。

## Local Decisions

- VFS 在 dimension replay 顺序中先于 projection-only 维度，原因是 Skill/guideline 等 projection 需要从 final VFS 派生。
- Provider/model 配置只暴露 `provider_id`、`model_id`、`thinking_level` 等业务参数，原因是业务层不应管理 provider tuning knobs。
- `CapabilityScope`（Project/Story/Task）定义在 SPI `tool_capability.rs`，由 `LifecycleSubjectAssociation`、`AgentFrame` 与 `PermissionGrant` 推导。
- `CapabilityContext.granted_capability_keys` 允许 Permission Grant 授予的 keys 绕过静态 visibility 规则。

## Grant-Aware Visibility Contract

`default_visible_capabilities` 在评估 well-known keys 可见性时遵循以下优先级：

```
1. Permission Grant Override (highest)
   if granted_keys.contains(key) → visible (skip static rules)

2. Static Visibility Rules
   is_capability_visible(cap, scope, agent_declares, workflow_declares) → bool
```

### Signatures

```rust
// crates/agentdash-application/src/capability/resolver.rs

pub struct CapabilityContext {
    pub subject_kinds: Vec<SubjectKind>,
    pub granted_capability_keys: BTreeSet<String>,
}

fn default_visible_capabilities(
    owner_ctx: &CapabilityScopeCtx,
    merged: &MergedToolInput,
    granted_keys: Option<&BTreeSet<String>>,
) -> BTreeSet<ToolCapability>;
```

### Data Flow

```
PermissionGrant(Applied)
  → grant.requested_paths.iter().map(|p| p.capability) → keys
  → inject into CapabilityContext.granted_capability_keys
  → CapabilityResolver::resolve() checks granted_keys first
  → capability visible regardless of CapabilityScope static rules
```

### Runtime Transition from Grants

Permission grants are applied via `RuntimeCapabilityTransition`:

```rust
// PermissionGrantCompiler::compile(grant)
CapabilityDeclarationRecord {
    dimension: CapabilityDimensionKey("tool"),
    declaration_type: "capability_directive",
    source: CapabilityArtifactSource::permission_grant(),
    payload: ToolCapabilityDirective::Add(path),
}
```

This transition enters the standard capability dimension pipeline (replay → reduce → project → update session tools).

### Wrong vs Correct

#### Wrong: Only checking static rules

```rust
// WRONG — ignores grants, so granted capabilities remain invisible
fn default_visible_capabilities(owner_ctx, merged) {
    for key in WELL_KNOWN_KEYS {
        if is_capability_visible(cap, owner_ctx.owner_type(), ..) {
            effective.insert(cap);
        }
    }
}
```

#### Correct: Grant override takes priority

```rust
// CORRECT — grants bypass static visibility
fn default_visible_capabilities(owner_ctx, merged, granted_keys) {
    for key in WELL_KNOWN_KEYS {
        if granted_keys.is_some_and(|gk| gk.contains(key)) {
            effective.insert(cap);
            continue; // skip static rules
        }
        if is_capability_visible(..) { effective.insert(cap); }
    }
}
```

## Contract Appendices

- [Tool Capability Pipeline](./tool-capability-pipeline.md)
- [Capability Dimension Pipeline](./capability-dimension-pipeline.md)
- [LLM Model Config](./llm-model-config.md)
- [Host Integration API](./integration-api.md)
- [Permission System](../permission/architecture.md)
