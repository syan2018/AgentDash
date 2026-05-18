# 设计：Runtime 稳定性与执行准入小修

## 边界

本任务只在现有架构内做局部增强，不新增跨层事实源。

- Relay registry 仍是在线连接与 pending request 的内存表。
- RoutineExecution 仍是 routine dispatch 审计记录，不代表完整 Agent run。
- Provider adapter 测试只补稳定性矩阵，不改变 executor 协议。

## 模块设计

### 1. Relay 错误分类

当前 `BackendRegistry::send_command` 返回 `anyhow::Error`，调用方只能看到中文文本。建议新增局部错误类型，例如：

- `BackendCommandError::Offline`
- `BackendCommandError::SendFailed`
- `BackendCommandError::Timeout`
- `BackendCommandError::ResponseDropped`

若短期调用面太宽，可先让错误类型实现 `std::error::Error` 并保持 `anyhow` 上抛，但内部测试直接断言具体 variant。后续 API 层可把 variant 映射为稳定错误码。

### 2. Routine 触发准入

准入分两类：

- 配置错误：Project、Agent、ProjectAgentLink、template 等缺失或非法，仍标记 failed。
- 环境暂不可用：已配置的 workspace/backend 当前不可用，标记 skipped。

`RoutineExecution.mark_skipped(reason)` 已存在可复用；若原因字段只能复用 `failure_reason`，需保持命名兼容并在 DTO/前端文案里说明 skipped reason。

Routine 成功 dispatch 后仍调用 `mark_completed()`，并保留“prompt 已派发”的语义注释。

### 3. Provider adapter 行为矩阵

优先检查 `crates/agentdash-executor/src/connectors` 现有测试，补齐最容易自动化的 fixture：

- provider 输出 session id 可捕获。
- usage 信息可解析或缺失时有明确行为。
- stderr tail / error classification 不吞掉关键诊断。
- timeout/cancel/drain timeout 有稳定事件或错误。
- poisoned output / resume fallback 先形成 checklist，避免本任务过度扩张。

## 风险

- relay 错误类型若直接改 public trait，可能牵动 API/workspace/vfs 调用面；应优先做兼容式引入。
- Routine skipped 判断不能把真实配置错误掩盖为 skipped。
- provider adapter 测试可能受第三方 CLI 行为影响，应以 fixture/mock 为主。
