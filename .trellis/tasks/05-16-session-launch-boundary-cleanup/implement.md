# Session 重构边角清理与 Launch 边界收敛实施计划

## Checklist

- [x] 读取相关 session spec 与现有测试模式。
- [x] 修复 `execute_constructed_launch` 中 claim 后早期错误不释放的问题。
- [x] 为 claim 后早期错误补充测试。
- [x] 调整 Project/Story/lifecycle node Plain owner construction，避免跳过 VFS/MCP/capability 事实解析。
- [x] 为 Plain owner construction 补充测试或断言。
- [x] 检查 local relay / task source MCP 语义，补齐明显遗漏。
- [x] 暂停继续小修，先完成 Construction / Launch 边界彻底瓦解评估。
- [x] 按一次性修复计划重构 final construction contract。
- [x] 运行验证命令。
- [x] 输出 Construction / Launch 边界彻底瓦解评估。

## One-shot Refactor Execution Plan

1. Final construction contract
   - 新增或扩展 construction resolution 字段，记录 VFS/MCP/capability/executor/working_directory 来源。
   - 新增 `SessionConstructionPlan::validate_for_launch()`，把 launch 前 facts gate 固化成代码。
   - 所有进入 LaunchPlanner 的 construction plan 必须是 final facts，不允许 partial plan。

2. ConstructionProvider 接管事实裁决
   - provider 在拿到 `LaunchCommand` 后，同时读取 meta、runtime entry、requested runtime commands、cached continuation、connector restore support。
   - provider 统一完成 lifecycle、owner context、Plain cleanup、local relay VFS/MCP、pending overlay、skills/guidelines discovery、working directory、capability_state 合成。
   - `default_vfs` 只能作为 construction provider 初始化依赖，不能进入 launch/prompt pipeline。

3. LaunchPlanner 瘦身
   - 删除 `default_vfs`、`vfs_service`、`extra_skill_dirs` 依赖。
   - 删除 VFS/MCP/capability fallback chain。
   - 删除 `SessionConstructionPlanner::plan_launch` 二次构建。
   - 保留 runtime-only 职责：prompt payload、lifecycle/restore/hook/follow-up、runtime command apply plan、terminal effect plan。

4. LaunchExecution 收口
   - source summary 从 construction resolution 读取。
   - connector input 的 working directory、executor_config、MCP、VFS、identity、capability 全部从 `SessionConstructionPlan` 投影。
   - `LaunchExecutionInput` 不再携带 construction fact 副本。

5. 验收锁定
   - production grep 禁止 Launch 层出现 construction fallback。
   - 单测覆盖 final plan validation、Plain owner cold follow-up、local relay construction-only、pending overlay、LaunchPlanner 无 fallback 能力。

## Implementation Notes

- `SessionConstructionPlan::validate_for_launch()` 已成为 launch 前 facts gate。
- `SessionConstructionProviderInput` 把 session meta、live runtime 状态、cached capability
  snapshot、requested runtime commands 交给 construction provider。
- API construction provider 现在负责 final VFS/MCP/capability/executor/working directory
  裁决、pending runtime command overlay、skills/guidelines discovery 与 resolution trace。
- `SessionLaunchPlanner` 已删除 VFS/MCP/capability fallback chain、skill/guideline discovery
  与 `SessionConstructionPlanner::plan_launch` 二次 construction。
- `default_vfs` 已从 session runtime/launch 装配中删除。

## Validation Commands

```powershell
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application session::construction
cargo test -p agentdash-application session::turn_supervisor
cargo test -p agentdash-application session::terminal_effects
cargo test -p agentdash-application session::hub
cargo test -p agentdash-api session_construction
rg -n "PreparedSessionInputs|finalize_request|finalize_augmented_request|SessionLaunchIntent|PromptSessionRequest" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

## Risky Files

- `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
- `crates/agentdash-application/src/session/prompt_pipeline.rs`
- `crates/agentdash-application/src/session/hub/tests.rs`

## Rollback Points

- claim 释放修复可单独回滚。
- Plain owner construction 改动应保持在 bootstrap use case 内，避免牵连 assembler 内部语义。
