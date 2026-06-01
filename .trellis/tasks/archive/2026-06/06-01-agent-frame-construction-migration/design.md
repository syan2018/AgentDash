# AgentFrame Construction 设计

## 目标

`AgentFrame` 是 `LifecycleAgent` 某个 revision 的 effective runtime surface。它回答“这个 agent 现在使用哪个 procedure、看见哪些 context、拥有哪些 capability、挂载哪些 VFS/MCP surface、对应哪些 RuntimeSession trace refs”。本任务把 `StepActivation`、`SessionConstructionPlan`、`LaunchPlan`、`HookSessionRuntime`、`CapabilityState` 收束到 frame builder 和 runtime adapter。

## 目标分层

| 层 | 目标职责 | 旧来源 |
| --- | --- | --- |
| `AgentFrameBuilder` | 解析 procedure、context、capability、VFS、MCP、ports、kickoff/delivery frame，产出 frame delta | `StepActivation`、session construction resolver、capability resolver |
| `AgentFrameConstructionPlan` | builder 内部临时结构，只服务 frame revision 生成 | `SessionConstructionPlan`、`ConstructionResolutionPlan` |
| `AgentFrame` revision | 持久事实源：effective surface + provenance + runtime refs | `CapabilityState`、context bundle、VFS/MCP projection |
| `RuntimeLaunchRequest` | 从 frame 投影出的 runtime adapter 请求 | `LaunchPlan`、`ConnectorInputPlan` |
| connector `ExecutionContext` | connector-facing DTO，继续存在但只由 `RuntimeLaunchRequest` 投影 | 当前 connector launch path |
| `AgentFrameHookRuntime` | hook 的 live/runtime facet，以 frame/agent 为主键，session 仅为 trace ref | `HookSessionRuntime`、`SessionHookSnapshot` |

## Frame revision 规则

以下变化必须产生新 revision 或 frame event：

- procedure 改变。
- capability / tool / MCP surface 改变。
- context slice 或 VFS/canvas surface 改变。
- runtime session refs 发生控制面意义变化。
- permission grant 或 context transition 改变 agent 可见面。

以下内容不应写入 frame：

- 完整 runtime transcript。
- Task / Story aggregate body。
- Activity attempt terminal result。
- connector 内部 transient state。

## Runtime launch 流程

```text
ExecutionIntent
  -> LifecycleDispatchService
  -> AgentFrameBuilder
  -> AgentFrame revision
  -> RuntimeLaunchRequest
  -> connector ExecutionContext
  -> RuntimeSession events
```

`RuntimeSession` 可以被创建、attach 或 continue，但它的 owner 语义不来自 session；它只是 frame 的 delivery/trace refs。

## Hook runtime 流程

Hook query 和 resolution 必须以 `run_id + agent_id + frame_id` 为主语：

- hook 读取 context/capability/VFS/MCP surface：从 `AgentFrame` 或 `AgentFrameHookRuntime` 读取。
- hook advance/resolution：使用 assignment 或 graph instance refs 推进 Activity，不从 session id 解析 run。
- session-indexed hook API 只能作为 RuntimeTrace adapter，必须立即反查到 frame/agent。

## 与 Assignment 的边界

`AgentFrame` 不说明自己正在完成哪个 attempt；它只提供 agent 的有效运行面。`AgentAssignment` 负责把 `agent/frame` 接到 `graph_instance_id + activity_key + attempt`。因此 frame 可以跨多个 attempts，也可以因为 context/capability 变化生成新 revision。

## 迁移策略

- 先保留 connector `ExecutionContext`，但把生成入口改为 frame projection。
- `SessionConstructionPlan` 不再生成 public contract，不作为业务模块 input。
- `CapabilityState` 的权威位置迁到 frame；session runtime command 只做 delivery queue。
- `SessionMeta`、live session maps、hook runtime 不能继续作为 effective surface 的平行权威。
