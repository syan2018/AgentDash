# Lifecycle mount 架构设计

## Current Architecture

AgentRun lifecycle surface 现在已经有 `AgentRunLifecycleSurfaceProjector`，但事实来源还没有完全收口。几个入口都会在各自上下文中重新组装一份 surface input：

- AgentRun workspace query 从当前 frame 与 execution anchor 生成 `WorkspaceReadSurface`，但没有把 anchor node facts 带入 projection。
- VFS surface resolver 在每次 list/read 时重新 resolve `surface_ref`，并从 anchor 拼出只读 node 信息；这里没有 `lifecycle_key` 和 writable port facts。
- owner bootstrap 在 Project owner 场景中根据 active workflow 推导 node facts，并用 `node_projection.is_some()` 决定 mode。
- session assembler 已经多处调用 projector，但 `SessionAssemblyBuilder` 和若干测试仍能直接调用低层 lifecycle mount builder。

这意味着当前代码的真实状态是“多个入口各自收集事实后重建 lifecycle mount”。`mode` 已经存在，但部分入口仍通过是否存在 node facts 间接决定 session evidence scope 或 node runtime scope。

## Current Data Flow

当前 lifecycle mount facts 分散在三层：

- execution anchor 提供 `runtime_session_id`、`run_id`、`agent_id`、`launch_frame_id`、`orchestration_id`、`node_path`、`node_attempt`。
- active workflow projection 提供 `lifecycle_key`、attempt、output port keys。
- SkillAsset projection 既来自显式/builtin skill 列表，也会从旧 lifecycle mount metadata 中读回已投影 keys。

mount metadata 也分成两类：

- provider 必需 metadata：`scope`、`run_id`、`runtime_session_id`、`orchestration_id`、`node_path`、`attempt`、`writable_port_keys`、`skill_asset_project_id`、`skill_asset_keys`。
- projector 调试 metadata：`agent_run_lifecycle_surface` 嵌套结构以及与 provider metadata 重复的扁平字段。

provider 必需 metadata 是 VFS provider 的读取、写入和 skills 目录解析契约；重复 projector metadata 不是 provider/API 的事实源。

## Target Architecture

目标架构将 AgentRun lifecycle surface 分成四个明确边界：

1. 入口层只收集上下文事实。

   AgentRun workspace query、VFS surface resolver、owner bootstrap、session assembler 不直接构造 mount，也不拼 mount metadata。它们调用 projector 的场景化子入口，例如 workspace read、launch evidence、node execution、companion child。

2. Projector 持有 typed projection facts。

   Projector 负责把 address、message stream、anchor node、workflow node、SkillAsset projection 和 builtin skill requirement 合成一份 typed facts。session evidence scope 与 node runtime scope 由 facts 的 surface kind 决定，不由某个 Option 是否存在隐式决定。

3. Builder 只接收最终 mount spec。

   低层 mount builder 不读取旧 mount metadata，不 ensure builtin skill，不判断业务入口。它只把已完成的 session evidence spec 或 node runtime spec 转成 `Mount`、`root_ref`、capabilities 和 provider 必需 metadata。

4. Projection refresh 与 overlay replace 分离。

   VFS overlay / mount directive 继续保持整 mount replace 语义；lifecycle projection refresh 负责在同一 Project 内完整重算 SkillAsset、message stream、node facts。SkillAsset projection 不再依赖从旧 mount metadata 回读作为事实源。

## Current vs Target

| Area | Current | Target |
|---|---|---|
| Business entry | 多个调用点可重建 lifecycle mount | 业务层只调用 projector 场景化子入口 |
| Scope selection | 部分路径由 node facts 是否存在推导 | surface kind 显式决定 session evidence 或 node runtime |
| Node facts | API resolver 可构造不完整 `OrchestrationNodeProjectionInput` | 只读 evidence ref 与可写 node runtime facts 分离 |
| SkillAsset projection | 从旧 mount metadata 回读并追加 | typed facts 一次性生成 provider metadata |
| Builder visibility | `build_*lifecycle_mount*` 仍 public 导出 | builder 降到 lifecycle surface/provider 内部 |
| Metadata | provider facts 与 projector debug facts 重复 | provider 必需 metadata 是唯一运行时契约 |
| Tests | 以 helper 和断旧测试为主 | 覆盖 workspace query、VFS resolver、node runtime、session evidence 的路径一致性 |

## Cleanup Candidates

Lifecycle scope 内优先清理：

- 删除 `build_lifecycle_mount` 和 `build_lifecycle_mount_with_ports` convenience wrapper，避免默认 attempt=1 的旧式调用继续扩散。
- 收束 `LifecycleMountSurface`、`append_active_workflow_lifecycle_mount`、`project_active_workflow_lifecycle_vfs`，让 active workflow 也通过 projector facts 进入 lifecycle surface。
- 拆分只读 node evidence 与可写 `OrchestrationNodeProjectionInput`，避免 VFS resolver 填空 `lifecycle_key` / `writable_port_keys`。
- 删除或降级 `AgentRunLifecycleMountMetadata` / `agent_run_lifecycle_surface` 嵌套 metadata，保留 provider 必需 metadata。
- 删除未使用的 `MessageStreamTraceKind::RestoredTranscript`，除非后续设计引入真实 restored transcript projection。

全局 legacy 清理候选作为本 task 下的 work items 维护：

- `agentdash-agent-protocol/src/compat/mod.rs` dead module。
- `ExecutionSource::Migration` 及其无构造点分支。
- `SessionCapabilityEntry::legacy` 旧 flat skill 工厂。
- `ContextBundle::render_section` deprecated public API。
- 前端 `SessionChatViewTypes` 中 RuntimeSession 创建/切换 props。
- `workspaceRouting.ts` 的 deprecated re-export。
- flat skills fallback、workflow contract 静默忽略旧字段、AgentSource alias、capability `workspace_module` 默认值。

## Non-Goals

- 不引入兼容路径或回退路径。
- 不把普通 diff/patch 语义中的 `old/new`、业务态 `deprecated`、命令并发 stale guard 当成 legacy 清理对象。
- 不把不同验证边界混成一次不可审查的大改；同一 Trellis task 内用 `work-items/` 文件维护拆分和依赖。
