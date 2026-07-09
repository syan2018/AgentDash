# 手动上下文压缩实际执行链路收敛实施计划

## Scope

本任务实现手动 context compaction 的 compact-only 执行链路收敛。优先改后端 Rust 逻辑和测试，不处理前端展示样式。若 API result payload 字段需要调整，直接更新调用方类型和测试。

## Steps

1. 增加 compaction eligibility 结构化分类。
   - 在 `crates/agentdash-agent/src/compaction/mod.rs` 中新增 eligibility enum 与诊断函数。
   - 保留或迁移 `should_execute_compaction` 调用方，最终让 preflight 使用结构化分类。
   - 添加单元测试覆盖 true noop 与三类 invariant failure。

2. 调整 preflight lifecycle。
   - 在 `crates/agentdash-agent/src/agent_loop/streaming.rs` 中根据 eligibility 分类发 noop 或 failed。
   - invalid input 走 `after_compaction_failed`，并携带稳定 reason code、message/ref count、manual metadata。
   - cancel/abort 的 manual request 也进入 failed finalization。
   - 让 preflight 返回 outcome；`agent_loop_compact_only` 将 failed outcome 映射为维护轮失败，普通 provider turn 保留现有可继续策略。

3. 收敛 compact-only restore path。
   - 在 `crates/agentdash-application-runtime-session/src/session/launch/planner.rs` 或 `resolve_prompt_launch_path` 调用处为 `LaunchSource::ContextCompaction` 增加 maintenance restore 规则。
   - 无 live runtime 且 session 有历史事件时强制 `RepositoryRehydrate(ExecutorState)`。
   - restored state 为空或 refs 不完整时产生失败诊断，不进入普通 noop。

4. 修正 command receipt 与 request 状态。
   - 在 `crates/agentdash-application-agentrun/src/agent_run/context_compaction_command.rs` 中保留 maintenance `turn_id`。
   - request `Failed` 时写入 command result `Failed`，带 request id、turn id、reason。
   - request `Noop` 时保留 turn id，message 只表达真实 no eligible。
   - 确保 `Completed` / `Noop` / `Failed` 都能从 request 状态、maintenance turn id 和 lifecycle item id 追到同一条 compact lifecycle。

5. 验证 projection commit 成功路径。
   - 补齐或新增应用层测试，用持久化 session events 构造可压缩 transcript，触发 compact-only，断言 compaction/projection/request 三组记录一致。
   - 运行最小相关测试集，再按风险选择更广的 workspace 检查。

## Verification Commands

- `cargo test -p agentdash-agent compaction`
- `cargo test -p agentdash-application-runtime-session context_compaction`
- `cargo test -p agentdash-application-agentrun context_compaction`

如果测试名过滤无法覆盖新增用例，改跑对应 package 的完整测试或更精确的 test target。
