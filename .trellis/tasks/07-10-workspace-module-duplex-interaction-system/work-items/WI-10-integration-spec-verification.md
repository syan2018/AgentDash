# WI-10 Integration / Spec Verification

Status: planned

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
