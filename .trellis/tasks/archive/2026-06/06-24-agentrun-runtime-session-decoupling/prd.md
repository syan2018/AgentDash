# AgentRun / RuntimeSession 边界解耦实施

## Goal

在 release 前将 session 收束为 RuntimeSession delivery/trace substrate，并把 AgentRun/Lifecycle current surface query/update、API consumer、RuntimeGateway、Canvas/WorkspaceModule/Permission 等路径迁出 session 横向耦合。

本任务是后续实际解耦实施的唯一收束任务。父任务中的所有解耦重构目标都收束为本 child 内部 work items，一口气完成迁移；不再为这些解耦目标创建独立 Trellis child。所有实施、检查和阶段状态都在本任务内追踪。

## Context

- 父任务：`.trellis/tasks/06-24-release-crate-boundary-review`
- 关键研究输入：
  - `research/01-session-runtime-inventory.md`
  - `research/02-agentrun-lifecycle-surface.md`
  - `research/03-api-runtime-gateway-consumers.md`
  - `research/04-business-surface-update-paths.md`
  - `research/05-crate-split-coupling-map.md`
- 目标边界：`session` 收束为 RuntimeSession delivery/trace/runtime coordination substrate；AgentRun/Lifecycle owns current runtime surface query/update、effective capability/admission、resource surface、AgentFrame write boundary 和 control-plane state。

## Requirements

- 收束 `session` public facade，不再从 `session` 对外分发 AgentRun/Lifecycle/frame/capability ownership 类型。
- 将 `AgentFrameRuntimeTarget`、active runtime surface adoption port、current surface query/update DTO 归属到 AgentRun 边界；RuntimeSession 只实现 live adoption adapter。
- 将 `session/launch/orchestrator.rs` 与 `session/launch/commit.rs` 中的 AgentFrame write、LifecycleAgent current delivery binding、bootstrap status decision 迁入 AgentRun/Lifecycle launch/commit adapter。
- 将 AgentRun current runtime surface query/update、effective capability/admission、resource surface query 固化为 public application facade；API、RuntimeGateway、Canvas、Extension、Terminal、VFS consumer 只消费 facade DTO。
- 将 RuntimeGateway-facing AgentRun surface/MCP access contracts 移入 `agentdash-application-ports`，RuntimeGateway providers 保持在 RuntimeGateway facade 后方。
- 将 `AgentRunFrameSurfaceService` 或等价 facade 变成 surface-changing business path 的唯一入口；Canvas、WorkspaceModule、Permission、MCP/VFS/Skill/AgentProcedure update 只提交 typed update request。
- 清理 API route-local anchor/current-frame/read-model 拼接：VFS AgentRun source latest-anchor selection、sessions/lifecycle views current frame resolver 等应迁到 application read-model facade。
- 补齐 Canvas / Extension runtime route 的 Project/session 一致性校验，防止 path Project 与 runtime session current surface Project 不一致。
- 压缩 application root、`session/mod.rs`、`agent_run/frame/mod.rs`、`vfs/mod.rs` 的 public exports，先形成未来 crate split 能接受的 import graph。
- 本任务不做物理 crate extraction；crate 拆分只消费本任务输出作为前置条件。

## Work Items

Detailed tracking docs live under `work-items/`; the implementation dependency graph is `parallel-dag.md`. Parent task decoupling goals are mapped in `parent-child-coverage.md`; those goals are internal work items of this migration task, not separate child tasks.

1. AgentRun current surface facade
   - 稳定 `AgentRunRuntimeSurfaceQueryPort` 和 DTO。
   - 修正 API DTO 同时携带 launch frame 与 current surface frame。
   - 增加 AgentRun resource surface query facade，移出 API `session_construction.rs` 中的 projector 拼装。

2. RuntimeSession substrate facade
   - 收紧 `session/mod.rs` exports。
   - 移出 `AgentFrameRuntimeTarget` / `AgentFrameHookRuntimeTarget` ownership。
   - 明确 RuntimeSession 对外只暴露 delivery/trace/turn/event/resume/debug/persistence use case。

3. Launch / commit ownership
   - 从 session launch orchestrator/commit 中迁出 AgentFrame revision write。
   - 从 session 中迁出 LifecycleAgent delivery binding 和 bootstrap status decision。
   - 保留 RuntimeSession accepted turn、trace、connector attach 和 stream processing。

4. Surface update ownership
   - 将 Canvas expose/bind、Permission grant apply/revoke、WorkspaceModule visibility、MCP preset、Project VFS mount、Skill inventory、AgentProcedure contract update 收敛到 AgentRun typed update facade。
   - Permission adapter 不再在 permission 模块公开持有 `AgentFrameBuilder`。
   - SessionHub adoption primitive 降级为 AgentRun update facade 内部 adapter。

5. RuntimeGateway / MCP access
   - 保持 `CurrentSurfaceRuntimeMcpAccess` 消费 AgentRun query port。
   - 抽出 gateway-facing port/DTO，避免 RuntimeGateway 直接依赖 AgentRun implementation details。
   - 增加静态或单元检查，防止 RuntimeGateway MCP access 回退到 SessionHub/AgentFrame resolver。

6. API consumer cleanup
   - Canvas runtime invoke/bridge 加 Project/session binding guard。
   - Extension runtime action/channel 加 path Project/session Project mismatch rejection。
   - Terminal launch target derivation 移到 application runtime placement facade。
   - VFS `SessionRuntime` / `AgentRun` source 统一走 AgentRun resource surface facade。
   - `routes/sessions.rs` / `routes/lifecycle_views.rs` 的 current frame read-model 迁到 application query facade。

7. Visibility and import cleanup
   - 降低 root `pub mod` / `pub use`。
   - 消除 production API 对 `session::construction_planner`、`session::plan`、`session::AgentFrameRuntimeTarget` 等路径依赖。
   - 降低 `agent_run <-> session`、`agent_run <-> lifecycle` 双向 import 热点。

8. Verification and regression coverage
   - 覆盖 Canvas idle `mcp.list_tools`、MCP call/list、Extension runtime backend target、Terminal launch、VFS SessionRuntime/AgentRun resource surface、Permission grant update/adoption、WorkspaceModule Canvas bind update。
   - 每一阶段保持 Rust compile 通过；触及 API contract 时运行对应 contract/type checks。

9. Canvas / Extension Project binding guard
   - Canvas runtime invoke/bridge 在 Gateway 调用前校验 Canvas Project 与 runtime session current surface Project 一致。
   - Extension runtime action/channel 在 provider 调用前校验 path Project 与 runtime session current surface Project 一致。

## Acceptance Criteria

- [ ] `parallel-dag.md` and every `work-items/WI-*.md` tracking document stays updated through implementation.
- [ ] `parent-child-coverage.md` shows every parent decoupling child task covered by this child task's internal work items.
- [ ] `session/mod.rs` 不再 public re-export AgentRun/Lifecycle ownership 类型；production API 不再依赖 session planner/current-frame internals。
- [ ] `AgentFrameRuntimeTarget` 和 active runtime surface adoption port 归属 AgentRun；SessionHub 只作为 RuntimeSession live adapter 实现该 port。
- [ ] RuntimeGateway MCP access、Canvas/Extension/Terminal/VFS/API current-surface consumer 全部通过 AgentRun current/resource surface facade，不直接读 SessionHub 或 current frame resolver。
- [ ] RuntimeGateway-facing AgentRun surface/MCP access contracts 已进入 `agentdash-application-ports`；RuntimeGateway provider 不依赖 AgentRun implementation internals。
- [ ] `AgentRunFrameSurfaceService` 或等价 facade 是 surface-changing business paths 的唯一 public entry；Canvas/WorkspaceModule/Permission 不直接拥有 `AgentFrameBuilder` 或 active adoption primitive。
- [ ] `session/launch` 保留 RuntimeSession delivery/trace/connector commit，AgentFrame/Lifecycle write 决策迁到 AgentRun/Lifecycle adapter。
- [ ] Canvas/Extension runtime route 显式拒绝 path Project/Canvas Project 与 runtime session current surface Project 不一致。
- [ ] VFS `SessionRuntime` / `AgentRun` resource surface、Terminal launch target、Extension backend placement 共享同一 AgentRun resource/current surface closure。
- [ ] `cargo check -p agentdash-application`、`cargo check -p agentdash-api` 通过；按触及范围补充并运行相关 Rust tests。
- [ ] 更新相关 Trellis spec 或本任务 `design.md`，说明 RuntimeSession substrate 与 AgentRun current surface 的依赖方向。
- [ ] `review-gate.md` 通过，且 `target-application-state.md` 与最终 production module graph 一致。

## Notes

- 本任务不做物理 crates 拆分；只完成拆分前必须具备的模块/依赖解耦。
- 父任务中的 `physical-crate-extraction-wave-1/2` 不属于本任务；它们只在本任务完成后消费最终 import graph。
