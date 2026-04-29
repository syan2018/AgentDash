# Companion Agent 去特化与回流一致性修复

## 背景

当前 companion 机制虽然功能可用，但存在明显“特化路径”与边界耦合：

1. `companion_respond` 在 parent 回流路径未严格使用 `request_id` 关联，存在误命中风险
2. parent 自动恢复时复用 child 的上下文片段（尤其 `vfs`），可能污染父会话
3. payload request/response type 关联校验不完整（部分路径只做弱校验）
4. companion 派发流程包含多步副作用（meta/run/binding），失败时缺少一致性收敛
5. `CompanionRequestTool` 仍保留部分历史穿透字段，语义冗余

## 目标

把 companion 收敛为“普通 session 派生”的一致路径，重点保证：

- 请求关联严格（`request_id` 不再宽松匹配）
- parent 恢复只使用 parent 自身运行态上下文（不继承 child 切片）
- companion 协议校验可追溯且可解释
- 关键副作用路径失败后可回滚或恢复到一致状态

## 范围

### T1: request/response 关联契约收敛

- 在 `CompanionSessionContext` 中持久化 request type（用于 response 校验）
- `try_complete_to_parent` 仅在 `request_id == dispatch_id` 时命中
- 未命中时提供可诊断错误（期望 dispatch_id）

### T2: parent 自动恢复上下文一致性

- child 回流触发 parent 自动恢复时，不再透传 child `vfs`
- parent 恢复请求依赖 parent 自身运行态（executor/mcp）与常规 pipeline 解析

### T3: companion payload 校验补全

- parent 回流路径按原 request type 做 response type 匹配校验
- 保持前向兼容：未知 type 仍可按现有策略处理

### T4: 副作用失败收敛

- companion dispatch 注册 `companion_context` 后，若 `start_prompt` 失败则恢复旧值
- `setup_companion_workflow` 在 run/binding 创建后的失败分支执行 best-effort 回滚（delete）

### T5: 去除历史冗余耦合

- 删除 `CompanionRequestTool` 中无实际来源的 `context_bundle` 穿透字段
- 删除对外 event details 中不必要的 MCP 暴露字段（仅保留派发语义）

## 验收标准

1. `companion_respond` 不能用错误 `request_id` 回流到 parent
2. parent 自动恢复路径不再读取 child `vfs`，且行为稳定
3. `request_type -> response_type` 校验在 companion 回流路径可生效
4. companion workflow setup 中的 run/binding 创建后失败不再留下明显脏状态
5. 相关改动通过 `cargo check` 与关键测试
