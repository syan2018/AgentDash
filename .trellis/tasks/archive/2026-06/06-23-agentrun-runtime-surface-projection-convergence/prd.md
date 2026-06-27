# AgentRun runtime surface 投影收束

## Goal

将 runtime surface / capability / skill / VFS / workspace module 相关更新收束到 AgentRun 本体驱动的统一投影链路。

业务来源只提交“发生了什么变化”的更新请求；上下文事实（identity、owner scope、AgentFrame、active VFS、runtime backend、permission/admission、skill discovery provider 等）必须由 AgentRun 当前状态统一解析，不允许 Canvas、WorkspaceModule、Permission、MCP、VFS 等业务路径自行零散重建。

## Problem Statement

当前代码中，多条业务路径会直接或间接重建 `CapabilityState`、调用 `AgentFrameBuilder::with_capability_state`、adopt 已持久化 frame revision，或手写 `SessionCapabilityProjectionInput`。这导致同一 AgentRun 的上下文事实在不同路径上不一致。

已确认的症状包括：

- `workspace_module_invoke` 的 Canvas host operation 分支会通过 Canvas mount refresh 间接触发 AgentFrame capability revision adopt，使普通 operation dispatch 产生 runtime capability/context frame 副作用。
- Canvas 相关 runtime transition 重跑 skill discovery 时使用 `identity: None`，导致依赖 user/group/org context 的 external integration skill 可能在更新后消失。
- live VFS skill merge 以 `file_path.contains("://")` 判断旧 skill 是否保留，容易把 URI 型 external integration skill 当作旧 VFS skill 丢弃。
- `CapabilityKeyDimensionDelta` 在 added/removed 为空时仍生成 section，造成“无能力 key 变更”的空 capability delta 展示。
- `CapabilityState` 同时承载 tool capability、VFS、skills、workspace module visibility、MCP surface 等多类 runtime surface，导致非能力 key 的状态变化被误展示为 capability update。
- Lifecycle Workflow / AgentProcedure 路径也是 AgentFrame 的主要生产来源：workflow AgentCall 会由 lifecycle dispatch materialize agent/frame/session/anchor，后续 lifecycle node composer 将 `AgentProcedureContract` 的 behavior/capability/context/hook contract 写入 pending frame。该路径属于合法 frame construction，但也必须纳入同一 AgentRun frame/surface 边界，避免成为另一套可绕过的 AgentFrame 写入体系。

## Requirements

- 建立一个 AgentRun 本体驱动的 runtime surface 更新入口。所有会改变 runtime surface 的业务路径必须通过该入口提交更新请求。
- 建立统一 AgentRun frame/surface command boundary，覆盖 AgentFrame 初始化、definition/contract 投影、accepted launch commit 和运行期 runtime surface 更新。该边界内部可以分 construction 与 update 两条 typed command，但业务模块不得绕过边界直接持有 `AgentFrameBuilder` 写 surface。
- 新入口必须从当前 AgentRun / AgentFrame / delivery runtime session / active turn 中解析完整投影上下文，不允许业务调用方自行拼装关键上下文字段。
- 业务来源只表达变化意图，例如 Canvas binding changed、Canvas mounted、workspace module visibility changed、permission grant applied、MCP preset changed、project VFS mount changed、skill inventory changed。
- `workspace_module_invoke` 必须回归 operation dispatch 语义，不直接写 AgentFrame capability revision，不直接 adopt runtime surface，不直接发 capability/context frame。
- Canvas domain mutation 与 runtime surface projection 必须分层：Canvas mutation 可触发更新请求，但实际 AgentFrame/runtime surface 变更由统一入口决定。
- skill discovery 的 identity、workspace、owner、provider 上下文必须来自统一 AgentRun projection context。
- skill baseline merge 必须按 provider/source/capability identity 处理，不得用 URI 字符串形态猜测 skill 来源。
- capability/context frame delta 必须按语义维度展示。无 capability key 增删时不得生成空 capability key delta；VFS/skill/workspace module/MCP 的变化不得伪装成 capability key 更新。
- 冗余链路必须一并清理：业务模块/API route 不再直接调用 runtime adoption primitive；Canvas expose/adopt、Permission route adopt、业务直接 `with_capability_state` 等旁路需要删除、私有化或迁移为统一入口内部实现。
- 保留有价值的底层 primitive 时，必须重新划定可见性和调用边界。例如 active-runtime adoption 可以保留为统一 update service 内部操作，但不得作为 Canvas、WorkspaceModule、Permission 或 API route 的业务入口。
- AgentFrame 写入角色必须显式分层：初始 frame / workflow node frame / accepted launch commit 属于 frame construction；Canvas、Permission、WorkspaceModule、MCP/VFS/Skill 等运行期变化属于 runtime surface update。Lifecycle Workflow / AgentProcedure 模块可以提供 construction contract 和 composer 输入，但不能在统一 construction/update 边界之外直接写 surface revision。
- `AgentProcedureContract` 对 capability、context、mount directive、hook rule 的贡献必须通过 frame construction / lifecycle node composer 统一投影到 AgentFrame；后续如果 AgentProcedure activation 或 contract source 需要改变 live surface，也必须提交 typed update/construction request，而不是由 lifecycle/workflow 业务模块自行拼完整 `CapabilityState`。
- 重构应在一个 Trellis 任务内分阶段完成，阶段之间保持代码处于可检查状态。
- 项目未上线，不需要保留旧兼容路径；发现错误边界时应直接收束到正确模型。

## Non-Goals

- 不重做前端 workspace panel 的交互设计。
- 不引入兼容旧 runtime surface 更新路径的并行 fallback。
- 不为历史 session event 做兼容性展示迁移。
- 不改变 external integration provider 的产品能力范围；本任务只确保其发现结果在 runtime surface 更新中被正确保留和重投影。

## Acceptance Criteria

- [ ] 代码中 runtime surface 更新路径有唯一归口；Canvas、WorkspaceModule、Permission/MCP/VFS 等业务模块不再直接构造完整 `CapabilityState` 并写入 AgentFrame revision。
- [ ] AgentFrame 初始化、AgentProcedure/ProjectAgent/companion definition 投影、accepted launch commit、runtime surface update 都通过统一 frame/surface command boundary 进入；内部保留 construction/update 分流，但外部业务模块不能直接写 frame surface。
- [ ] 生产代码中的 AgentFrame 写入点被分类为 frame construction / launch commit / runtime surface update 三类；Lifecycle Workflow / AgentProcedure 写入只能落在 frame construction 边界内，不能作为业务旁路直接写 runtime surface。
- [ ] workflow AgentCall materialization 与 lifecycle node composer 的 AgentProcedure contract 投影仍能创建有效 AgentFrame / RuntimeSessionExecutionAnchor，并且相关写入点被纳入统一边界检查。
- [ ] `workspace_module_invoke` 的所有 operation dispatch 分支不会直接调用 `adopt_persisted_agent_frame_revision`、`expose_canvas_mount_revision_and_adopt` 或等价 capability revision adopt 入口。
- [ ] Permission approve/revoke 的 API route 和 permission service 不再直接掌控 active-runtime adoption；surface-changing grant 通过统一 AgentRun runtime surface update 入口写入并同步 runtime。
- [ ] Canvas expose/adopt 旧 helper 被删除、私有化或改造成统一入口内部 adapter；业务调用点只能提交 typed update request。
- [ ] Canvas binding/update 后，external integration skill discovery 已暴露的 skill 不会因为 identity 缺失或 URI path merge 规则被移除。
- [ ] runtime transition 触发 skill discovery 时，provider 能拿到与 AgentRun 当前身份一致的 user/group/workspace context。
- [ ] `CapabilityKeyDimensionDelta` 只在 capability key added/removed 非空时生成；空 “no change” capability key section 不再出现。
- [ ] VFS、skill、workspace module visibility、MCP 等变化拥有明确 semantic delta 或明确不展示策略，不再被误报为 capability key 更新。
- [ ] 新增/更新回归测试覆盖：external integration skill → Canvas update → skill 仍存在；workspace module invoke 不产生空 capability key delta；Canvas mutation 不直接绕过 AgentRun projection 入口。
- [ ] 相关设计被记录到本任务 `design.md`，执行步骤记录到 `implement.md`，并在进入实现前完成 review。
- [ ] 质量检查覆盖 Rust 编译/相关单元测试；如触及前端 context frame 展示，补充对应 TS/React 测试。

## Confirmed Evidence

- `workspace_module_invoke` Canvas host 分支会在 `canvas.bind_data` 后刷新 Canvas mount，并进入 capability/session runtime 相关路径。
- `SessionCapabilityService::derive_skill_entries_for_active_vfs` 当前以 `identity: None` 调用 `derive_session_skill_baseline`。
- owner bootstrap 路径会把 `spec.identity` 传入 skill baseline，说明初始构建与 runtime transition 的上下文事实不一致。
- `merge_live_vfs_skill_entries` 当前用 `file_path.contains("://")` 过滤旧 skill。
- `CapabilityKeyDimensionDelta::from_delta` 当前无条件返回 section。
- 子 Agent 研究确认：Permission grant apply/revoke、API route adopt、Canvas expose/adopt、workspace module Canvas invoke/present、companion selected ProjectAgent skill baseline 是当前主要散落路径；VFS fs/shell tools、MCP discovery、hook runtime 暂未发现直接写 AgentFrame revision，主要作为 consumer 依赖上游 surface 是否统一。
- 主 Agent 补充核查确认：`FrameConstructionService::compose_pending_frame`、`LifecycleDispatchService::materialize_workflow_agent_node`、`LifecycleDispatchService::create_initial_frame` 和 `session/launch/commit` 是 AgentFrame 的核心生产/提交路径；其中 AgentProcedure 通过 lifecycle node composer 提供 contract 输入，应作为合法 construction 写入边界被纳入收束。

## Research Tasks

- 子 Agent 正在调研更多散落路径，输出位置：`research/scattered-runtime-surface-paths.md`。
- 研究结果将作为 `design.md` 的输入，特别是统一入口边界、现有调用点迁移顺序、风险等级和测试补点。

## Open Questions

- 统一入口命名与模块归属：倾向放在 `agent_run` 或 `session` control-plane，而不是 Canvas/WorkspaceModule 下。
- runtime surface 更新请求是否立即写 AgentFrame revision，还是允许聚合到下一 turn 前统一应用；当前 MVP 倾向先保持即时写入，但由统一入口集中处理。
- workspace module visibility 是否保留在 `CapabilityState.workspace_module` 内，还是作为独立 runtime surface 维度长期拆出；当前 MVP 先收束更新入口，后续在设计中决定拆分程度。
- Lifecycle Workflow 的 `AgentProcedureContract` 如果在运行中发生变更，首期是否只影响下一次 node/frame construction，还是需要支持 active AgentRun surface update；当前 MVP 倾向先把已有 construction 写入边界收束清楚，live contract update 作为 typed request 能力预留。
