# 手动上下文压缩实际执行链路收敛

## Goal

手动触发 context compaction 时，系统必须基于当前 runtime session 的 durable model context 实际执行压缩。若上下文恢复或 `MessageRef` 边界无法满足结构性压缩契约，命令结果要暴露为明确的失败诊断；只有在上下文已经成功 materialize 且确实没有足够可压缩消息时，才返回 `no_eligible_messages`。

## Background

已查询数据库确认，最新手动请求 `cd970a80-357d-4d2d-a0d8-9a9fc31febcf` 在 `t1783593592078` 被 consumed 后 77ms 内产生 `context_compaction_noop`，metadata reason 为 `no_eligible_messages`，没有写入 compaction / projection records。用户贴出的 `kind = context_compaction` payload 是 compact-only maintenance turn 的 system delivery 事件，不是压缩结果。

用户明确指出：恢复上下文缺失或 `message_ref` 不完整而落到 `no_eligible_messages` 本身就是路径错误，需要统一评估。

## Requirements

- 手动 compact-only 维护轮的输入必须来自 `ContextProjector` 构建出的模型上下文，和普通冷启动续跑使用同一套 durable transcript / projection 恢复语义。
- `no_eligible_messages` 只表达真实业务结果：已恢复出完整模型上下文，但消息数量、keep-last 边界或 token budget 导致没有可压缩前缀。
- `message_refs.len() != messages.len()`、cut boundary ref 缺失、first-kept ref 缺失、恢复后 transcript 意外为空等结构性问题，要进入失败诊断和 request failed 状态。
- `runtime_session_compaction_requests`、command receipt、session event lifecycle 三者必须表达同一个最终结果；维护轮不能因为短轮询看到 `Noop` 就把结构性失败伪装成成功接受。
- running-session 的 next-turn 手动压缩和 idle/completed/interrupted session 的 compact-only 维护轮要共享 eligibility 判断和 request 生命周期语义。
- cancel / abort 期间已经 consumed 的手动请求必须被终结为可诊断状态，避免长期停留在 consumed 或在 UI 中显示为“已触发但没有结果”。
- 本项目处于预研期，修复以正确契约为准；涉及状态枚举、payload、migration 或测试夹具时直接更新到目标形态。

## Out Of Scope

- 不调整前端视觉展示；本任务只保证后端命令结果和事件状态可被正确消费。
- 不为旧状态做兼容回退；按当前预研期要求直接收敛到正确状态机。
- 不改变自动压缩的策略阈值；只修复手动触发链路与共享 eligibility 语义。

## Acceptance Criteria

- [ ] 对一个有足够历史消息和完整 `MessageRef` 的 idle/completed runtime session 手动触发 compact-only 后，会生成 `runtime_session_compactions`、`runtime_session_projection_segments` 和 `runtime_session_projection_heads`，并发出 `context_compacted` lifecycle。
- [ ] 同一请求对应的 `runtime_session_compaction_requests.status` 为 `completed`，带有 `completed_compaction_id`、`compacted_until_ref` 和 `first_kept_ref`；command receipt 的 result 指向本次 maintenance turn。
- [ ] 当恢复上下文为空、messages 与 refs 长度不一致、压缩边界 ref 缺失时，手动请求进入 `failed`，result metadata 包含稳定 reason code 和上下文计数；不会返回 `no_eligible_messages`。
- [ ] 当上下文完整但消息数量不足或 keep-last 规则保留全部消息时，手动请求进入 `noop`，reason 为 `no_eligible_messages`。
- [ ] compact-only launch plan 在无 live Pi runtime 但 session 有历史事件时恢复 executor state，而不是只投递 `"Run AgentDash manual context compaction maintenance turn."` 这条 system delivery。
- [ ] 摘要生成期间 cancel / abort 时，已 consumed 的手动请求进入可观测终态，command receipt 和 session diagnostic 能追到同一个 request id。
- [ ] 单元测试覆盖 eligibility 分类、manual delegate request 状态、compact-only restore path；至少一个应用层集成测试覆盖 manual compact-only 从历史事件到 projection commit 的成功链路。

## Technical Notes

- 相关技术设计见 `design.md`。
- 具体实施顺序见 `implement.md`。
