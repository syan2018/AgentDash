# W1: Mailbox Intake Command Shape

Status: implemented

## Goal

为 Routine / Companion 建立中性的 mailbox intake command 形态，复用现有 command receipt、delivery policy、scheduler 和 projection，同时接收 W0 定义的开放式 source identity，避免语义上把系统/agent 消息伪装成 user composer。

## Dependencies

- W0 source identity model 完成。

## Deliverables

- [x] 评估是否将 `accept_user_message_for_target` 抽出中性 command 名称，或保留现名但新增 Routine / Companion helper。
- [x] 明确 origin、source_identity、payload retention、executor_config、client_command_id 的生成规则。
- [x] 为 Routine / Companion message 建立 stable source dedup key 格式，优先引用 source_ref / correlation_ref。
- [x] 给后续 W2-W6 提供统一 helper 或清晰 application service entry。

## Implementation Notes

- `AgentRunMailboxIntakeCommand` / `AgentRunMailboxIntakeTargetCommand` 是中性的 intake entry，显式接收 `origin`、`MailboxSourceIdentity`、payload retention、`executor_config`、`client_command_id`、可选 `source_dedup_key`、`delivery_intent`、`identity` 和 `schedule_on_submit`。
- 现有 user/composer API 继续走 `AgentRunMailboxUserMessageCommand` / `accept_user_message`，内部转调 neutral intake，保持外部行为不变。
- source dedup 约定优先从 `MailboxSourceIdentity.source_ref` / `correlation_ref` 派生；source identity 没有稳定 ref 时使用显式 `source_dedup_key`；二者都没有时回落到 command receipt dedup。

## Acceptance

- [x] Routine / Companion 可复用 mailbox command receipt、policy、scheduler、projection。
- [x] 新 helper 不引入 direct launch、runtime-only notification queue 或平行 pending queue。
- [x] W2-W6 不需要各自复制 dedup / policy / command receipt 逻辑。

## Suggested Validation

- `cargo test -p agentdash-application-agentrun mailbox`

## Validation Results

- `cargo fmt --check` passed.
- `cargo check -p agentdash-application-agentrun` passed.
- `cargo check -p agentdash-api` passed.
- `cargo test -p agentdash-application-agentrun mailbox` passed.
- Trellis channel `check` worker dispatch was attempted for this wave, but the local Codex provider resolved to `node app-server` and failed before reviewing code. Main session completed the same scoped validation locally; future dispatches should avoid this provider configuration until the channel adapter is fixed.

## Parallel Guidance

W1 应在 W0 之后独占执行。W2-W6 应等 W1 的 helper shape 稳定后再并行，否则会在 mailbox application service 上产生重复封装。
