# Session LaunchPlan 阶段化 Implement

## Order

1. 阅读：
   - `.trellis/spec/backend/session/architecture.md`
   - `.trellis/spec/backend/session/session-startup-pipeline.md`
   - `docs/reviews/2026-05-16-zip-static-review/session-launch-refactor-plan.md`
2. 盘点当前入口：
   - HTTP prompt
   - Task step
   - Workflow AgentNode
   - Routine executor
   - Companion
   - Local relay prompt
3. 新增阶段类型草案：
   - `LaunchPlan`
   - `LaunchPlanner`
   - `TurnPreparation`
   - `ConnectorLaunch`
   - `TurnCommit`
4. 先迁移最窄入口，推荐 HTTP prompt 或 local relay prompt。
5. 调整 bootstrap/turn commit 顺序，补 failure rollback 逻辑。
6. 补测试矩阵。
7. 更新 session spec。

## Validation

```powershell
cargo test -p agentdash-application session
cargo check -p agentdash-application -p agentdash-api -p agentdash-local
```

重点测试：

- owner bootstrap 首轮与第二轮；
- connector.prompt 失败；
- 并发 prompt claim；
- local relay prompt；
- terminal event ingestion。

## Rollback Points

- 保留旧 facade，逐入口迁移。
- 新 `LaunchPlan` 初期可只覆盖已迁移入口，避免一次性改所有 caller。
