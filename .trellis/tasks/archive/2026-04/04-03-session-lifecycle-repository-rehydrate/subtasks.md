# 子任务拆分

## ST-01 生命周期判定收敛

- 目标：
  - 把 owner 首轮初始化、冷启动恢复、热续跑统一下沉到 session 生命周期模型
- 关键点：
  - 不允许 route/前端各自复制判定逻辑
  - `SessionPromptLifecycle` 成为唯一事实源

## ST-02 仓储历史重建

- 目标：
  - 从 `session_events` 恢复完整消息历史，而不是只拼一段 continuation 文本
- 关键点：
  - user / assistant / tool_call / tool_result 均可重建
  - owner resource block 必须过滤

## ST-03 Connector 恢复能力建模

- 目标：
  - 区分“connector 支持原生恢复”和“只能 continuation 文本兜底”
- 关键点：
  - `supports_repository_restore(executor)`
  - `has_live_session(session_id)`
  - `ExecutionContext.restored_session_state`

## ST-04 持久化补齐

- 目标：
  - 保证 `bootstrap_state` 在内存仓储、SQLite、PostgreSQL 和 migration 层都一致
- 关键点：
  - PostgreSQL 不能只依赖 repository `initialize()` 动态补列

## ST-05 前端真实回归

- 目标：
  - 用真实前端流程验证 reopen / restart 后不重复 bootstrap
- 关键点：
  - 首次 prompt
  - reopen 后 prompt
  - restart 后 reopen 再次 prompt

## ST-06 后续缺口记录

- 目标：
  - 记录本轮验证里暴露但不属于 session 生命周期主线的问题
- 当前缺口：
  - discovery 可见的远程 executor 与 session prompt 路由能力未对齐
