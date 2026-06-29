# W8: Workspace Projection And UX

Status: implemented

## Goal

让 Routine 与 Companion mailbox messages 在 AgentRun workspace 的同一 mailbox/status 面可观察，并复用 promote/delete/resume 等现有 mailbox commands。

## Dependencies

- W0 source identity model 完成。
- W2-W6 至少完成后端 message creation 与 source identity 写入。

## Deliverables

- [x] Workspace mailbox/status projection 根据 source identity 的 namespace/kind/display_label_key 展示 Routine / Companion label、preview、queued/blocked/paused 状态。
- [x] command set 继续复用 mailbox promote/delete/resume。
- [x] frontend 消费 generated mailbox contracts。
- [x] 增加 minimal view tests 或 typecheck 覆盖 Routine / Companion message。

## Acceptance

- [x] 用户能在 AgentRun workspace 看到 Routine 后续触发和 Companion sub / parent / human 回流。
- [x] 用户能用同一套 mailbox 操作暂停、恢复、删除、重排可重排消息。
- [x] projection 区分 composer、Canvas、Routine、Companion 各来源，而不改变 scheduler authority。

## Suggested Validation

- `pnpm run contracts:check`
- `pnpm run frontend:check`

## Validation Results

- `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check` passed.
- `.\\packages\\app-web\\node_modules\\.bin\\vitest.CMD run packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.test.tsx` passed.
- `.\\packages\\app-web\\node_modules\\.bin\\tsc.CMD --noEmit -p packages/app-web/tsconfig.app.json` passed.
- `pnpm run frontend:check` blocked before typecheck by local pnpm `ERR_PNPM_IGNORED_BUILDS` for `esbuild@0.27.3`; direct TypeScript and targeted component tests passed.

## Parallel Guidance

W8 的 label / generated contract 检查可以在 W0 后预备；最终 UI 验收必须等 W2-W6 后端路径稳定。不要让 W8 反向定义 source identity schema。
