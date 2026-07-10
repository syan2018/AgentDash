# WI-10 Integration / Spec Verification

Status: done

Depends On: WI-01 至 WI-09

## Scope

- 全量 affected-package checks、contracts、frontend/browser、migration。
- spec Invariants/Current Baseline/appendix 收敛。
- residual static scans、PRD acceptance review、tracker closure。

## Exit Criteria

- 所有 PRD acceptance criteria 有代码/测试/spec 证据。
- 旧 Canvas aggregate/runtime state、Session-bound contract、重复 Workspace Module provider、旧 Extension Channel 词汇清理完成。
- migration forward/clean database 与 repository concurrency 通过。
- V1 discriminator、future V2/migration policy、public identity 与 pin/retention static contract checks 通过。
- work items 经全量 gate 后统一标记 done。

## Validation

- 根 `implement.md` 第 3 节全部命令与检查类别。
- `task.py validate`、`git diff --check`。

## Final Evidence

- migration guard、test-support guard、contracts check、workspace check、strict clippy、shared/frontend/
  desktop typecheck、frontend 536 tests 与 Extension focused tests 通过。
- Rust 全量测试完成编译，并定位到 `agentdash-agent/tests/runtime_alignment.rs` 的 6 个既有 provider
  error-return 断言失败；本任务未修改 Agent loop。
- frontend lint 中本任务新增 Canvas 文件已 focused clean；仓库仍有 33 个既有 React effect lint。
- critical Story E2E 到达 Task 创建界面后因既有 `Not Found` 失败；页面快照确认 Interaction/Canvas
  并非失败路径。
- canonical Operation、旧 Canvas/Session-bound gateway 与 Extension runtime identity 静态扫描完成。
