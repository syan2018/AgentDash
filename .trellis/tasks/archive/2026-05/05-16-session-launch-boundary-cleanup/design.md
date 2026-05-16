# Session 重构边角清理与 Launch 边界收敛设计

## 背景

当前 session 主链路已经收敛为：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext -> connector
```

但 review 发现两个边角风险：

1. Project/Story owner 在 `Plain` 生命周期直接走 `apply_plain_lifecycle_request`，不会重新构建 owner VFS/MCP/capability。若进程冷启动但 meta 内仍有 executor follow-up session id，LaunchPlanner 会回退到 cached profile/default VFS。
2. `SessionLaunchExecutor::execute_constructed_launch` 在 `claim_prompt` 后读取 meta/runtime command 失败时直接 return，未释放 `Claimed`。

## 目标边界

Construction 应负责：

- owner 解析；
- workspace / working directory；
- VFS / MCP / capability state；
- context bundle / continuation frame；
- prompt blocks / env / executor config；
- source trace。

Launch 应负责：

- turn id / claim / activate；
- lifecycle / restore / hook / follow-up plan；
- runtime command apply plan；
- connector input projection；
- accepted 后的 meta/runtime-command/title/context-frame 副作用；
- terminal effect outbox。

## 根因评估：边界没有瓦解的真正位置

当前主链路名义上已经是：

```text
LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> connector
```

但实际代码里仍存在第二条隐性 construction：

```text
SessionConstructionPlan(partial)
  -> SessionLaunchPlanner
     -> 选择 VFS 来源
     -> 推导 working_directory
     -> 选择 executor_config 来源
     -> 打 pending capability overlay
     -> 选择 MCP 来源
     -> 合成 capability_state
     -> discovery skills / guidelines
     -> SessionConstructionPlanner::plan_launch(...)
  -> SessionConstructionPlan(final)
```

这导致 `SessionLaunchPlanner` 同时承担两种职责：

- Construction facts 的最终裁决者：VFS/MCP/capability/executor/working directory。
- Launch runtime plan 的生成者：lifecycle、restore、hook、runtime command、terminal effect。

边界没有真正瓦解的核心原因是：`SessionConstructionProvider::build_construction`
现在允许返回一个“半成品 plan”，再由 `LaunchPlanner` 用 `default_vfs`、
cached continuation、local relay workspace root、session meta executor 等来源补齐。
这让 Construction 和 Launch 之间形成了事实源竞争。

### 当前残留职责清单

| 残留点 | 当前所在 | 正确归属 | 风险 |
|---|---|---|---|
| VFS 来源裁决：construction / local relay / cached profile / hub default | `launch_planner.rs` | Construction | 冷启动 Plain follow-up 容易拿 cached/default surface |
| working directory 从 VFS default mount 推导 | `launch_planner.rs` | Construction | Launch 仍在解释 workspace 结构 |
| executor_config 从 construction / user input / meta 裁决 | `launch_planner.rs` | Construction | executor 来源 trace 不统一 |
| MCP 来源裁决：construction / local relay / cached profile / pending | `launch_planner.rs` | Construction + Runtime overlay | local relay/task source MCP 容易丢失或重复 |
| base capability_state 合成 | `launch_planner.rs` | Construction | capability 不是单一事实源 |
| skills/guidelines discovery | `launch_planner.rs` | Construction | connector 输入前仍有上下文发现副作用 |
| `SessionConstructionPlanner::plan_launch` 二次构建 plan | `launch_planner.rs` | 删除 | 明确说明第一次 construction 不完整 |
| `default_vfs` 注入到 `SessionLaunchDeps` | `launch_service.rs` / `prompt_pipeline.rs` | Construction provider 初始化依赖 | Launch 层保留 fallback 能力 |

## 一次性修复目标

一次性修复的目标不是“把两个类合并”，而是建立硬边界：

```text
Source adapter
  -> LaunchCommand
  -> ConstructionProvider
     -> SessionConstructionPlan(final construction facts)
  -> LaunchPlanner
     -> LaunchExecution(runtime-only plan)
  -> LaunchExecutor
     -> connector + accepted-side effects
```

`SessionConstructionPlan` 进入 `LaunchPlanner` 时必须已经满足：

- `workspace.working_directory = Some(...)`
- `execution_profile.executor_config = Some(...)`
- `surface.vfs` 按来源完成最终裁决
- `projections.mcp_servers` 是 connector 应看到的 base MCP 集合
- `projections.capability_state = Some(...)`，且内部 `vfs.active` 与 `tool.mcp_servers`
  与 plan 的 VFS/MCP 一致
- `projections.session_capabilities` 已根据最终 VFS discovery 得到
- `prompt.prompt_blocks/env/executor override` 已规范化
- `context.bundle/continuation_context_frame` 已按 lifecycle 处理好是否注入
- trace 记录 VFS/MCP/capability/executor/working_directory 的来源

`LaunchPlanner` 只能读取这些 facts，不再从其它来源补齐它们。

## 一次性修复计划

### Phase A：定义 Final Construction Contract

1. 扩展 `SessionConstructionPlan` 或新增 `ConstructionResolutionPlan`：
   - `vfs_source`
   - `mcp_source`
   - `capability_source`
   - `executor_source`
   - `working_directory_source`
   - `pending_overlay_applied: bool`
2. 增加 `SessionConstructionPlan::validate_for_launch()`：
   - 缺少 owner / working_directory / executor_config / capability_state 直接返回错误；
   - capability_state 内的 `vfs.active` 必须等于 `surface.vfs`；
   - capability_state 内的 `tool.mcp_servers` 必须等于 `projections.mcp_servers`；
   - 不允许 LaunchPlanner 再通过 cached/default 补齐。
3. 更新 spec：Construction 输出的是 final facts，不是 seed 或 partial plan。

### Phase B：把所有 facts 裁决搬入 ConstructionProvider

1. `SessionConstructionProvider::build_construction` 接收完整 launch 环境：
   - `LaunchCommand`
   - `SessionMeta`
   - `had_existing_runtime`
   - `requested_runtime_commands`
   - `cached_continuation`
   - connector capability query（是否支持 repository restore）
2. API provider 内部统一执行：
   - lifecycle 解析；
   - owner/context composer；
   - Plain lifecycle 清 context bundle，但保留 VFS/MCP/capability；
   - local relay workspace root -> VFS；
   - pending runtime capability transition overlay；
   - skills/guidelines discovery；
   - final working directory 推导；
   - final capability_state 合成；
   - final MCP set 合成。
3. 删除 `LaunchCommand` 上给 LaunchPlanner 使用的 local relay fallback 读取路径；
   local relay 仍由 source adapter 表达来源事实，但只被 ConstructionProvider 消费。

### Phase C：瘦身 LaunchPlanner 为 Runtime Planner

1. 从 `SessionLaunchDeps` 删除：
   - `default_vfs`
   - `vfs_service`
   - `extra_skill_dirs`
2. 从 `launch_planner.rs` 删除：
   - `discover_skills`
   - `discover_guidelines`
   - VFS fallback chain
   - MCP fallback chain
   - capability base state synthesis
   - `SessionConstructionPlanner::plan_launch`
3. `LaunchPlanner::plan` 改为：
   - 调 `construction.validate_for_launch()`；
   - resolve prompt payload；
   - resolve lifecycle/restore/hook/follow-up；
   - build runtime command apply plan；
   - build terminal effect plan；
   - call `LaunchExecution::build` with the original final construction plan。

### Phase D：收紧 LaunchExecution 输入

1. `LaunchExecutionInput` 删除 construction fact 字段：
   - `base_capability_state`
   - `vfs_source`
   - `mcp_source`
   - `capability_source`
   - `pending_vfs_overlay_applied`
   - `discovered_guidelines`
2. 改为从 `SessionConstructionPlan` 读取：
   - final capability state；
   - final sources；
   - discovered guidelines/session capabilities；
   - connector input facts。
3. `LaunchSummary` 的 source 字段来自 construction resolution trace，而不是 LaunchPlanner enum。

### Phase E：删除旧边界残留和测试锁死

1. 删除 `SessionConstructionPlanner::plan_launch` 和 `SessionConstructionLaunchInput`。
2. production grep 必须无：
   - `default_vfs` 出现在 launch/prompt pipeline；
   - `SessionConstructionPlanner::plan_launch`；
   - `LaunchVfsSource::CachedSessionProfile` 这类 Launch fallback source；
   - `LaunchMcpSource::CachedSessionProfile`；
   - `LaunchCapabilitySource::Default`。
3. 增加 contract tests：
   - Construction final plan 缺字段时 LaunchPlanner 拒绝；
   - Plain owner follow-up 冷启动仍产出 owner VFS/MCP/capability；
   - local relay 的 workspace root 和 MCP 只由 construction 消费；
   - pending runtime command overlay 只在 construction 形成 final facts，connector accepted 后才 mark applied；
   - LaunchPlanner 无 default/cached fallback 能力。

## 本任务已做的小修应如何纳入一次性方案

已完成的边角修复不应作为终点，只能视为 Phase E 前的风险止血：

- `claim_prompt` 后早期错误释放 claim：保留，属于 LaunchExecutor runtime cleanup。
- Plain owner 不再短路空 construction：保留，但后续应改成 final construction contract 的一部分。
- Task source MCP 合并：保留，但后续应进入统一 MCP resolution，而不是散落在 assembler 调用点。

这些补丁可以随一次性重构一起提交，也可以在开始大重构前先保留为绿色基线。

## 非目标

- 不保留兼容 fallback。
- 不引入旧 prepared/finalize request 路径。
- 不让 LaunchPlanner 读取 cached profile/default VFS 来补齐 construction facts。
- 不把 prompt execution 副作用搬进 Construction；claim/activate/connector/accepted-side
  persistence 仍属于 LaunchExecutor。

## 验收门槛

- `SessionLaunchPlanner` 内不再出现 VFS/MCP/capability/executor fallback chain。
- `SessionConstructionPlan::validate_for_launch()` 是 launch 前唯一 facts gate。
- `LaunchExecution::build` 对 connector input 的所有 session facts 都来自
  `SessionConstructionPlan`。
- `rg -n "default_vfs|plan_launch|CachedSessionProfile|LaunchCapabilitySource::Default" crates/agentdash-application/src/session` 对 production launch path 无命中。
- `cargo test -p agentdash-application session::`
- `cargo test -p agentdash-api session_construction`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`

## 本任务落地范围

### 1. Plain owner construction 不再短路为空计划

对 Project/Story 的 `Plain` 生命周期：

- 仍调用 owner composer，得到完整 VFS/MCP/capability/context projection；
- 再清空本轮不应重复注入的 bootstrap bundle/continuation；
- 保留 `prompt_blocks`、executor config、surface/projections。

Task path 已经始终调用 `compose_story_step_prompt`，本任务重点补齐 Project/Story/lifecycle node 的短路。

### 2. claim 后错误释放

在 `execute_constructed_launch` 中把 claim 之后、planner 成功之前的 fallible 步骤纳入统一释放逻辑：

- meta 不存在；
- meta store error；
- runtime command store error；
- planner error。

释放策略使用 `TurnSupervisor::clear_turn_and_hook`，保持与 planner error / connector error 一致。

### 3. 边界彻底瓦解落地

本任务已把 LaunchPlanner 内的 construction facts 裁决一次性迁回 ConstructionProvider：

- 扩展 `SessionConstructionPlan` 的 `resolution` 子结构，记录 VFS/MCP/capability/executor/working directory 来源；
- 将 VFS/MCP/capability/executor fallback source、pending overlay、skills/guidelines discovery 移入 construction；
- LaunchPlanner 只处理 runtime facts：lifecycle、hook session、restore transcript、follow-up、runtime command apply plan 与 terminal effect plan。

## 风险与回滚

- Project/Story Plain 续跑改为仍跑 composer，可能增加一次 VFS/capability 解析成本；这是正确性优先的可接受成本。
- 若 composer 对 Plain 路径产生重复 bundle，需要显式清空 `context.bundle`，只保留 capability/surface/projection。
- claim 释放变更局部，失败路径回滚简单。

## 验证策略

- 单测覆盖 owner Plain construction 完整性。
- 单测覆盖 claim 后 meta missing/runtime command error 释放。
- 运行 session 相关测试。
- 运行 production grep，确保旧 request/finalizer 未回流。
