# Session Refactor Batch 2b：Launch Fallback Extraction

## Goal

继续收缩 `start_prompt_with_follow_up` 的执行前策略解析，把 lifecycle / restore / follow-up / pending / VFS-MCP-capability 来源摘要下沉到 `LaunchExecution`。本批不迁移入口，不删除 `PromptSessionRequest`，也不搬运行时副作用；目标是为 Batch 3 的 `LaunchCommand` adapter 迁移清出足够稳定的 execution boundary。

## Requirements

- `LaunchExecution` summary 覆盖：
  - prompt lifecycle；
  - repository restore mode；
  - hook reload trigger；
  - follow-up session source；
  - pending transition apply count；
  - VFS source；
  - MCP source；
  - capability source；
  - working dir input 与 resolved path。
- `start_prompt_with_follow_up` 中同类字段只能由一个 plan/summary 描述，不再散落无法审计的隐式来源。
- pipeline 仍负责副作用：
  - turn reservation；
  - hook runtime reload / refresh；
  - skill/guideline discovery；
  - tool building；
  - event append；
  - connector prompt；
  - processor spawn。
- Batch 0 characterization 必须继续通过。

## Acceptance Criteria

- [x] `LaunchExecution` 单测覆盖 VFS/MCP/capability 来源摘要。
- [x] `LaunchExecution` 单测覆盖 follow-up session id 来源摘要。
- [x] `LaunchExecution` 单测覆盖 pending transition count 与 repository restore mode。
- [x] `start_prompt_with_follow_up` connector prompt 前的 summary 能解释关键 fallback 来源。
- [x] pending apply-once 与 connector setup failure 回归测试通过。
- [x] 不新增入口 adapter，不删除 `PromptSessionRequest`。

## Completion Notes

- `LaunchSummary` 已扩展 restore mode、follow-up source、VFS/MCP/capability source、pending VFS overlay、working dir input/resolved path。
- `prompt_pipeline.rs` 现在在 connector prompt 前同步生成 fallback 来源摘要，并把 follow-up session id 解析提前到 `LaunchExecution` 输入之前。
- 本批没有迁移入口 adapter，也没有改变 pending/effects/runtime 存储语义。

## Out of Scope

- 不实现 `LaunchCommand` production adapters。
- 不删除 `SessionLaunchIntent`。
- 不迁移 pending runtime command storage。
- 不拆 terminal effects。
