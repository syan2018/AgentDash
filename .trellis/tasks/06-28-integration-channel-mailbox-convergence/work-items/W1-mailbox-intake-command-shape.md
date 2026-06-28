# W1: Mailbox Intake Command Shape

Status: planned

## Goal

为 Routine / Companion 建立中性的 mailbox intake command 形态，复用现有 command receipt、delivery policy、scheduler 和 projection，同时避免语义上把系统/agent 消息伪装成 user composer。

## Dependencies

- W0 source/schema baseline 完成。

## Deliverables

- [ ] 评估是否将 `accept_user_message_for_target` 抽出中性 command 名称，或保留现名但新增 Routine / Companion helper。
- [ ] 明确 origin、source、identity、payload retention、executor_config、client_command_id 的生成规则。
- [ ] 为 Routine / Companion message 建立 stable source dedup key 格式。
- [ ] 给后续 W2-W6 提供统一 helper 或清晰 application service entry。

## Acceptance

- [ ] Routine / Companion 可复用 mailbox command receipt、policy、scheduler、projection。
- [ ] 新 helper 不引入 direct launch、runtime-only notification queue 或平行 pending queue。
- [ ] W2-W6 不需要各自复制 dedup / policy / command receipt 逻辑。

## Suggested Validation

- `cargo test -p agentdash-application-agentrun mailbox`

## Parallel Guidance

W1 应在 W0 之后独占执行。W2-W6 应等 W1 的 helper shape 稳定后再并行，否则会在 mailbox application service 上产生重复封装。

