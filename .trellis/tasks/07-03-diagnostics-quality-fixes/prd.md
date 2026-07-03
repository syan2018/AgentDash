# 收束 diagnostics 调用点质量

## Goal

基于本轮诊断日志审计结果，收束公开主仓现有 `diag!` / `diag_error!` 调用点质量，让后端关键失败在 `/api/diagnostics`、JSON 日志和 stdout 中都能携带稳定的 `operation`、`stage`、错误对象和可检索上下文字段。

## Requirements

- 仅修改公开主仓内容，不引入任何私有域名、内部服务名、私有配置或企业身份字段。
- 不重新设计 diagnostics facade；遵循 `.trellis/spec/backend/diagnostics-guidelines.md` 的既有标准。
- 将带错误对象的 `diag!(Warn/Error, ..., "{error}")` 或拼接错误消息优先升级为 `diag_error!`。
- 每个错误诊断必须有稳定 `operation` 和 `stage`，并通过结构化字段补充排障必要事实，例如 `session_id`、`run_id`、`backend_id`、`project_id`、`workflow_id`、`command_id`、`attempt`、`retry_count`。
- 日志消息保留人类可读摘要，但不要把所有信息塞进 message；查询依赖结构化字段。
- 不记录密钥、token、完整命令参数、环境变量原文、用户隐私内容、大段 stdout/stderr 或模型输出正文。
- 修复范围优先覆盖审计中列出的高风险路径：relay/ws、本地后端通信、AgentRun 执行、session launch/runtime、stream、Postgres runtime、VFS/API 错误入口。
- 保持现有业务逻辑、错误返回语义和公开 API 合同不变。
- 必须保留并运行 clippy 裸 `tracing::*` 守门；不得用新的裸事件宏绕过 `diag!`。

## Acceptance Criteria

- [ ] `rg -n "tracing::(info|warn|error|debug|trace)!" crates` 在主仓 workspace 中没有新增裸事件宏，已存在豁免必须有明确原因。
- [ ] 审计文件中标为 P0/P1 的 `diag!(Warn/Error)` 质量候选已经逐项处理或在任务 notes 中记录不处理原因。
- [ ] 关键错误调用点使用 `diag_error!`，且包含 `DiagnosticErrorContext::new("<operation>", "<stage>")`。
- [ ] `diag_error!` 调用点补充至少一个可关联上下文字段；没有上下文可用时，需要在 review 中说明。
- [ ] `cargo fmt --check` 通过。
- [ ] `cargo check` 覆盖被改 crate；若运行全 workspace 成本过高，至少覆盖所有被改 Rust crate。
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 或项目脚本 `pnpm run backend:clippy` 通过；若因既有无关问题失败，记录失败点和已覆盖的替代检查。

## Notes

- 这个任务从诊断日志审计过程中拆出，但自身必须可独立进入公开主仓 PR。
- 任务聚焦“日志质量补齐”，不包含任何私有集成调用点。
