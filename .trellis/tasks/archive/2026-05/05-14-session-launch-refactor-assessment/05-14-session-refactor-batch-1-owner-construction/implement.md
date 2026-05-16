# Implementation Plan：Batch 1 Owner + Construction

## Steps

1. 搜索当前 owner/context 组装路径，定位 launch augment 与 context query 的 owner priority 分叉。
2. 新增 `session/ownership`：
   - `ResolvedSessionOwner`
   - `SessionOwnerResolutionTrace`
   - `SessionOwnerResolver`
   - priority 单元测试。
3. 将 API context query 的 primary binding 选择改为调用 owner resolver。
4. 更新 Batch 0 context priority characterization：目标态应断言 Task -> Story -> Project。
5. 新增 `session/construction` 的最小 plan/projection 类型和 builder。
6. 将 `/sessions/{id}/context` 的 owner/context projection 接入 construction plan。
7. 保留既有 project/story/task context builder 作为迁移 adapter，并记录后续下沉边界。
8. 运行 focused tests，并根据失败补齐 owner/construction 测试。
9. 完成 Trellis check，提交 Batch 1。

## Candidate Commands

```powershell
cargo test -p agentdash-application session::ownership
cargo test -p agentdash-application session::construction
cargo test -p agentdash-api acp_sessions
cargo fmt --check
```

## Commit Plan

```text
feat(session): 引入统一 owner 解析

- 新增 ResolvedSessionOwner 与 SessionOwnerResolver。
- 统一 context query 与 launch augment 的 owner priority。
```

```text
feat(session): 引入 construction plan 投影

- 新增 SessionConstructionPlan 与 context projection。
- 将 session context route 改为投影 construction fact。
```

如果实现中两个边界紧密耦合，可合并为一个提交，但提交说明必须同时列出 owner 与 construction 两个事实源变化。

## Exit Criteria

- owner priority 分裂消失。
- context endpoint 与 launch construction 有同一 owner 事实源。
- context endpoint response 由 construction projection 输出。
- project/story/task context builder 下沉边界已明确，避免在 Batch 1 牵动所有 route 复用点。
- 没有新增空壳 service 或多余传递层。

## Verification

```powershell
cargo test -p agentdash-application session::ownership
cargo test -p agentdash-application session::construction
cargo test -p agentdash-api acp_sessions
cargo fmt --check
```

以上命令均通过；`agentdash-application` 仍有既有 `canvas/management.rs` unused import warning，非本批引入。
