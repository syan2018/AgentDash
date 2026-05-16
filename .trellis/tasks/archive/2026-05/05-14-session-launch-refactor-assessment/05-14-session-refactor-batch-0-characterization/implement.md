# Implementation Plan：Batch 0 Characterization

## Steps

1. 读取相关现有测试，确认哪些行为已有覆盖。
2. 补齐 `assembler.rs` 中 `finalize_request` 的合并语义测试。
3. 补齐 `path_policy.rs` 中绝对路径与 `..` 当前行为测试。
4. 在 API 或 application 测试中固定 owner priority 现状差异。
5. 评估 `hub/tests.rs` 中 pending transition / connector failure 覆盖；若已有覆盖，记录即可；若缺关键断言，则补齐最小断言。
6. 运行 focused tests。
7. 更新父任务或子任务 notes，说明 Batch 0 已固定的行为。

## Candidate Commands

```powershell
cargo test -p agentdash-application session::assembler
cargo test -p agentdash-application session::path_policy
cargo test -p agentdash-application session::hub
cargo test -p agentdash-api acp_sessions
```

如果测试模块名与 cargo filter 不匹配，以实际模块路径调整。

## Commit Plan

推荐提交：

```text
test(session): 固化现有启动链路行为

- 补齐 request assembly、path policy 与 prompt pipeline fallback/failure 的 characterization 覆盖。
- 记录当前不安全或待迁移行为，供后续批次有意修改。
```

如果 API owner priority 测试独立且文件较多，可拆第二个提交：

```text
test(api): 固化 session context owner 选择现状

- 覆盖 context query 当前 Project -> Story -> Task priority。
- 标记 Batch 1 引入单一 owner resolver 时需要有意更新。
```

## Exit Criteria

- focused tests 通过。
- 新增测试不要求生产架构重构。
- Batch 0 没有引入 `SessionConstructionPlan` / `LaunchExecution` / launch service 空壳。

## Verification

```powershell
cargo test -p agentdash-application session::path_policy
cargo test -p agentdash-application session::assembler::tests::finalize_request_tests
cargo test -p agentdash-api pick_primary_session_binding_currently_prefers_project_story_task
cargo test -p agentdash-application pending_capability_state_transition_applies_on_next_prompt_and_clears_meta
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
```

以上 focused tests 均通过。
