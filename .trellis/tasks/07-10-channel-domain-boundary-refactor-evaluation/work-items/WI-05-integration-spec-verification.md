# WI-05 Integration / Spec Verification

Status: planned

Depends On: WI-01 至 WI-04

## Scope

- 全量 Rust/TS/contracts/SDK/relay/local/frontend/docs 集成。
- database/capability/runtime-gateway/mailbox/cross-layer spec 收敛。
- residual static scans、migration 与 PRD acceptance review。

## Exit Criteria

- 所有 PRD acceptance criteria 有代码/测试/spec 证据。
- ExtensionProtocol 旧词汇、synthetic channel identity、admission bypass 清理完成。
- owner-local persistence 与 reverse index 合同通过并发/恢复验证。
- work items 经全量 gate 后统一标记 done。

## Validation

- 根 `implement.md` 第 3 节全部检查类别。
- `task.py validate`、`git diff --check`。
