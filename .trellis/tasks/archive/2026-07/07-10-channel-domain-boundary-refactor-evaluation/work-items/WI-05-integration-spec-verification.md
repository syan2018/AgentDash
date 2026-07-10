# WI-05 Integration / Spec Verification

Status: done

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

## Final Evidence

- repository static scans：旧 Extension Channel vocabulary、旧 Channel domain variants、synthetic production Channel identity 与 unsupported production resolver 均为 0 hit；Mailbox/Gate materializer 只接受 `AdmittedChannelDelivery`。
- Rust：9 个受影响 packages `cargo check` passed；domain Channel 11、application Channel 15、Companion 22、AgentRun projection 1、API provider registration 3 tests passed；relay/local/runtime-gateway/workspace-module test targets `--no-run` passed。
- Quality：`cargo fmt --all -- --check` passed；受影响 backend packages strict clippy passed（仅放行 workspace 既有 large-enum/question-mark style lints）。
- Cross-layer：Extension 37 tests + typecheck、app-web typecheck + 12 focused tests、generated contracts check、migration guard 与 Trellis context validate passed。
