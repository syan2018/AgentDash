# WI-13 AgentRun Surface Authority 收束

Status: planned

Depends On: WI-11、WI-12、Agent Runtime 已接受 Surface revision 合同

## Problem

当前至少有 `CapabilityState`、`AgentRunAppliedResourceSurface`、`ActorOperationSurface`、
`PlatformToolBroker` definitions 与 `ResolvedWorkspaceModuleSurface` 分别解释 AgentRun 当前可见、
可调用和已应用的能力。消费者还会自行选择 binding frame、latest frame、applied resource surface
或动态 provider 结果，因此同一个 AgentRun 可能出现原生工具可调用、对应 Operation 不可见、
Workspace Module 列表为空的分裂状态。

动态 provider 解析失败目前还可能被降级为空 catalog，使“成功解析且没有能力”与“Surface
无法完成解析”无法区分。运行中能力修改也没有统一经过 AgentRun Surface adoption，容易演化成
Workspace Module、Operation 和 Broker 各自更新一部分状态。

## Authoritative Model

本工作项沿用 Agent Runtime 已冻结的四阶段语义：

```text
Product AgentFrame desired surface
  -> Complete Agent offered surface
  -> Host bound surface
  -> concrete Agent applied receipt
```

- AgentFrame 保存 Product 期望的业务能力；只有已被 canonical `SurfaceAdopt` 接受的 revision
  可以成为 current desired surface。
- offer、bound 与 applied receipt 证明具体 Agent 实际可以接纳什么，不成为第二套 Product
  capability authority。
- Product binding、resource grants、provider readiness 与 applied evidence 由同一次解析读取，
  并携带各自 revision/digest，禁止混用不同时间点的 frame 与 receipt。
- 运行中变更继续经过 AgentRun `SurfaceAdopt` command；本项消费并收束该入口，不建立
  Workspace Module 或 Operation 专属的 grant 状态机。

## Target Module

在 AgentRun application 层建立深模块 `AgentRunSurfaceAuthority`。其外部 interface 保持为
current surface resolve；Surface mutation 复用 canonical adoption use case：

```rust
trait AgentRunSurfaceAuthority {
    async fn resolve_current(
        &self,
        target: &AgentRunTarget,
    ) -> Result<ResolvedAgentRunSurface, AgentRunSurfaceResolveError>;
}
```

`ResolvedAgentRunSurface` 是 request-scoped、不可持久化、带 revision/digest 的解析结果。模块
内部统一规范化 capability、resource grant、provider readiness 与 applied evidence，并从同一
结果生成以下投影：

```text
ResolvedAgentRunSurface
  -> RuntimeToolProjection
  -> ActorOperationProjection
  -> WorkspaceModuleProjection
  -> SurfaceDiagnostics
```

这些投影是模块内部行为，不形成供消费者重新组合的公共状态袋。ToolBroker 继续拥有每次执行的
permission、effect 与 resource authorization；Surface Authority 只向它提供同源的 current
admission input。

## Capability And Visibility Contract

- `CapabilityState` 中显式 capability 与 enabled cluster 在 Surface 编译时只规范化一次，生成
  canonical capability set；Runtime tools、Operation descriptors 与 Workspace Modules 全部消费
  该集合。
- 原生 builtin module 从已准入的 native tool definitions 与 explicit Operation exposure registry
  推导。`WorkspaceModuleDimension` 负责 extension/canvas 等模块选择，不再通过调用方内联
  `module.kind == Builtin` 特判形成第二套可见性规则。
- `OperationRef`、descriptor、effect/replay、schema 与 provenance 继续来自 canonical
  OperationGateway catalog；Workspace Module 只组织同一次 actor projection。
- invocation 时重新解析 current surface 并经过 OperationExecutionCore 与 PlatformToolBroker，
  discovery revision 不成为 bearer capability。

## Currentness And Failure Contract

- current frame 指 canonical Surface adoption 已接受的 revision，而不是 repository 中任意最新写入
  revision。
- binding、frame、bound surface 与 applied receipt 不一致时返回 typed stale/unavailable，不选择
  其中一份继续拼装。
- 每个动态 facet 显式表达 `ready / unavailable`；provider discovery 失败返回结构化 diagnostic，
  只有成功解析后的空集合才表示当前确实没有相应能力。
- Surface adoption 无法热应用时返回 typed `requires_rebind` 或 apply failure，由统一 adoption
  recovery 收敛；调用方不会观察到只更新了 Module、Operation 或 Broker 的中间状态。

## Replacement Scope

- `ApplicationWorkspaceModuleRuntimeToolService` 改为消费 Workspace Module projection，不再自行
  读取 binding/frame/resource surface、过滤 capability 或吞掉 provider failure。
- `ApplicationSurfaceOperationAuthority` 改为消费 Actor Operation projection，不再独立读取
  latest AgentFrame 或构造 capability grant。
- `ProductPlatformToolOperationAccess` 与 Product runtime tool authorizer 消费同一 Broker
  admission projection，保持执行时重新授权。
- VFS、Task 与 Complete Agent runtime surface adapters 消费 Runtime Tool projection。
- 删除被替代的重复 resolver、builtin 特判、capability mapper 与独立 current-frame 读取；原
  `AgentRunAppliedResourceSurfaceQueryPort` 若只剩内部 facet，则收回模块内部。

## Integration Contracts

- `07-20-agent-runtime-persistence-authority-convergence` 已冻结 desired/offer/bound/applied 的事实
  归属；本项只统一解析与投影，不引入新的 durable Surface owner。
- `07-16-runtime-surface-provenance-permission-cleanup` 拥有 canonical `SurfaceAdopt` 顺序、active
  revision 与 AgentRun permission facade；本项复用其结果，不复制 adoption 或 permission policy。
- WI-12 的 explicit platform tool exposure registry 保持 Operation 暴露白名单；Surface
  Authority 负责让已授权的原生工具、Operations 与 builtin modules 使用同一 capability 事实。

## Write Set

- `crates/agentdash-application-agentrun/src/agent_run/`
- `crates/agentdash-application/src/runtime_tools/`
- `crates/agentdash-application/src/workspace_module.rs`
- `crates/agentdash-application-operation-gateway/src/operation/`
- `crates/agentdash-api/src/bootstrap/runtime_gateway.rs`
- `crates/agentdash-agent-runtime/src/platform_tool_broker.rs`
- 相关 backend/cross-layer specs、Skill 与父任务规划文档

实际写入前先与 07-16、07-20 的当前 diff 核对，重叠文件只沿其已建立的 authority seam 接入。

## Exit Criteria

- [ ] 一个 AgentRun target 只通过 `AgentRunSurfaceAuthority` 解析 current capability、resource、
      provider readiness 与 applied evidence。
- [ ] 同一 accepted frame 中的 VFS/Task/Process 能力会同时产生 native runtime tools、exact
      `platform:*` Operations 与对应 `builtin:*` modules；能力移除后三者同时消失。
- [ ] enabled cluster 与 explicit capability 不再由不同消费者分别解释；规范化后的 canonical
      capability set 具有 focused contract tests。
- [ ] Workspace Module、Operation authority、PlatformTool access 与 runtime tool adapters 不再
      独立读取 latest frame、拼装 applied surface 或内联 builtin visibility 特判。
- [ ] provider/binding/applied receipt 失败返回 typed unavailable diagnostic，不表现为成功的空
      Workspace Module catalog。
- [ ] 未被 SurfaceAdopt 接受的最新 frame 不会成为 current；revision/digest 不一致可验证地拒绝。
- [ ] 运行中 Surface 变更只经 canonical adoption；无法热应用时返回明确 rebind/apply 结果，
      不产生局部可见状态。
- [ ] direct invoke 与 OperationScript nested invoke 继续在执行时重新进入 OperationExecutionCore
      和 PlatformToolBroker authorization。
- [ ] 被替代的浅 resolver、重复 mapper、旧 tests 与静态依赖被删除；新测试只通过深模块 interface
      验证可观察结果。
- [ ] `workspace-module-system` Skill 与相关 specs 说明同源 Surface、typed unavailable 和
      Surface revision 变更方式。

## Validation

- AgentRun Surface Authority interface contract tests。
- Workspace Module + platform Operations + Broker 组合测试。
- accepted/latest frame、applied digest mismatch 与 provider unavailable tests。
- `cargo test` / `cargo check` 覆盖受影响 crates。
- repository-wide `rg` 检查消费者中的独立 frame/resource/capability 解析残留。
- `git diff --check`。
