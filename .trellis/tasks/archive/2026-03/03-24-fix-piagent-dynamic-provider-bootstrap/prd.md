# 修复 PiAgent provider 动态初始化

## Goal
让设置页保存 LLM Provider 后，无需重启后端即可让 `PI_AGENT` 发现到最新 provider / model 配置。

## Requirements
- 后端启动时即使 system scope 还没有任何 LLM provider，也要保留 `PI_AGENT` 的动态接入能力。
- 保存新的 `llm.*` 设置后，discovery 必须能基于最新 settings 返回 provider / model 列表。
- 若用户移除了最后一个 provider，后端不能继续回退到旧的启动期 provider 缓存。
- 未配置任何 provider 时，`PI_AGENT` 执行应给出明确错误，而不是依赖重启或静默使用陈旧配置。

## Acceptance Criteria
- [ ] 在“启动时无 provider”的场景下，后续写入 settings 后无需重启即可发现模型。
- [ ] 清空 provider 配置后，discover 返回空 provider 列表，不继续泄露旧模型。
- [ ] 针对上述场景存在回归测试。

## Technical Notes
- 优先收敛在 `crates/agentdash-executor/src/connectors/pi_agent.rs`。
- 保持现有前端 `refreshKey` 方案不回退，仅补齐后端缺失的动态注册能力。
