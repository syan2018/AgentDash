# Capability Architecture

## Role

Capability 子系统统一描述 session 能力声明、runtime transition、工具/MCP/VFS/companion/skill/guideline/extension 等维度的投影闭包。它防止能力知识散落在各 session 创建路径和 runtime update 分支中。

## Invariants

- 所有 session 声明式工具基线由 `CapabilityResolver` 或 capability dimension pipeline 统一计算，不在 session 创建路径硬编码；运行期最终可见能力由 AgentRun effective capability/admission 输出。
- `ToolCapability` 是开放 string key，不是封闭枚举。
- runtime transition payload 保存 declaration/effect records，不保存完整 `CapabilityState`、runtime surface、Skill baseline 或 guidelines projection。
- built-in dimension module 必须在 replay 前 decode 并 validate 自己的 typed payload。
- 新能力维度通过 dimension module 注册进入 validation、replay 和 projection normalize，不扩展中心化 transition input struct。
- 工具 schema 暴露入口必须消费 AgentRun final visible capability view；工具执行入口必须消费 AgentRun admission decision。

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
- `CapabilityScope`（Project/Story/Task）定义在 SPI `tool_capability.rs`，由 `LifecycleSubjectAssociation` 与 AgentRun 当前 `AgentFrame` 推导。
- AgentRun effective capability/admission 是 runtime 能力读取唯一入口。`PermissionGrant` 是 AgentRun-scoped 授权/护栏系统，只在 AgentRun 服务内部投影为 final visible capability 或 admission decision。执行态 admission 以 `runtime_session_id` 为入口，原因是 delivery runtime session 是连接 active tool invocation、AgentRun runtime anchor、anchored frame surface 与 frame-scoped grant projection 的最小稳定坐标。

## AgentRun Effective Capability Contract

运行时工具、MCP、VFS、WorkspaceModule、hook runtime 与 extension admission 都从 AgentRun effective capability/admission 服务取值。该服务组合以下输入：

- AgentRun 执行坐标：run、LifecycleAgent、delivery runtime、runtime anchor 指向的 AgentFrame。
- AgentFrame model-visible surface：`effective_capability_json`、`vfs_surface_json`、`mcp_surface_json`、visible canvas/workspace module refs。
- AgentRun-scoped Grant system：工具内部能力准入、Agent 工具集拓展请求、审批/撤销/过期生命周期。
- Runtime policy / command policy / admission state。

Grant effects are classified by AgentRun:

| Grant effect | AgentRun projection | AgentFrame revision |
| --- | --- | --- |
| Tool-internal capability permission | admission decision only | no surface revision |
| Agent toolset expansion | final visible capability | writes a model-visible surface revision |

### Signatures

```rust
use agentdash_application::session::AgentFrameRuntimeTarget;
use agentdash_spi::{CapabilityState, RuntimeMcpServer, ToolCapability, Vfs};
use std::collections::BTreeSet;
use uuid::Uuid;

pub struct AgentRunEffectiveCapabilityRequest {
    pub runtime_session_id: String,
    pub agent_run_id: Uuid,
    pub agent_id: Uuid,
    pub command_key: Option<String>,
}

pub struct AgentRunEffectiveCapabilityView {
    pub target: AgentFrameRuntimeTarget,
    pub capability_state: CapabilityState,
    pub visible_capabilities: BTreeSet<ToolCapability>,
    pub vfs_surface: Vfs,
    pub mcp_surface: Vec<RuntimeMcpServer>,
    pub visible_workspace_module_refs: Vec<String>,
}

pub struct AgentRunAdmissionDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

pub struct AgentRunAdmissionRequest {
    pub runtime_session_id: String,
    pub capability_key: String,
    pub tool_name: String,
    pub cluster: Option<ToolCluster>,
}
```

### Data Flow

```
AgentRun request
  → select runtime session anchor + anchored AgentFrame
  → load AgentFrame model-visible surface
  → project AgentRun Grant system into admission/toolset effects
  → return final visible capability view or admission decision
  → downstream modules consume only the AgentRun output
```

RuntimeSession launch may carry the adapter that calls `AgentRunEffectiveCapabilityPort::admit_tool`,
but that adapter is only an execution bridge. It maps the assembled tool schema provenance to
`AgentRunAdmissionRequest` and short-circuits before `tool.execute` when AgentRun denies. This keeps
the Grant decision at AgentRun while letting the agent loop remain the single physical execution entry.

### Runtime Transition from Grants

Permission grants can produce `RuntimeCapabilityTransition` only for toolset expansion that changes model-visible surface:

```rust
// AgentRun grant classifier output for a toolset expansion grant
CapabilityDeclarationRecord {
    dimension: CapabilityDimensionKey("tool"),
    declaration_type: "capability_directive",
    source: CapabilityArtifactSource::permission_grant(),
    payload: ToolCapabilityDirective::Add(path),
}
```

This transition enters the standard capability dimension pipeline (replay -> reduce -> project -> write AgentFrame revision -> adopt runtime). Tool-internal grant effects stay in AgentRun admission projection.

### Wrong vs Correct

#### Wrong: Reading grants inside the resolver

```rust
fn default_visible_capabilities(owner_ctx, merged, active_grants) {
    // Coupling: resolver now understands Grant lifecycle and bypass rules.
    let granted_keys = active_grants.iter().flat_map(|g| g.requested_keys());
    resolve_static_visibility(owner_ctx, merged).union(granted_keys)
}
```

#### Correct: AgentRun owns final view and admission

```rust
fn runtime_tool_surface(agent_run_id, agent_id) {
    let view = agent_run_capabilities.effective_view(agent_run_id, agent_id)?;
    view.visible_capabilities
}

fn invoke_tool(agent_run_id, command) {
    let decision = agent_run_capabilities.admit(agent_run_id, command)?;
    if !decision.allowed { return Err(...); }
    run(command)
}
```

## Contract Appendices

- [Tool Capability Pipeline](./tool-capability-pipeline.md)
- [Capability Dimension Pipeline](./capability-dimension-pipeline.md)
- [LLM Model Config](./llm-model-config.md)
- [Host Integration API](./integration-api.md)
- [Permission System](../permission/architecture.md)
