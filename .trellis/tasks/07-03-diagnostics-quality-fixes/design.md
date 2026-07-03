# 设计说明

## 背景

主仓已经有标准化 diagnostics facade：

- `diag!`：平台过程诊断唯一入口。
- `diag_error!`：带错误对象的标准错误诊断入口。
- `DiagnosticErrorContext`：统一提供 `operation`、`stage` 和派生 `detail`。
- workspace `clippy.toml`：禁止裸 `tracing::{info,warn,error,debug,trace}!` 回退。

当前主要缺口不是“有没有统一入口”，而是部分调用点虽然已经走 `diag!`，但仍把错误压成 message 字符串，缺少稳定操作名、失败阶段和可检索字段。结果是用户看到错误时，后端 diagnostics 很难回答“哪个操作、哪一步、携带哪些关联 id、原始 error/debug 是什么”。

## 修复策略

1. 保留 `diag!` 处理普通生命周期、分支决策和无错误对象的提示。
2. 对所有持有 `Error` / `anyhow::Error` / `Box<dyn Error>` / `Result::Err(e)` 的 Warn/Error 调用，优先升级为 `diag_error!`。
3. 给每个升级点定义稳定命名：
   - `operation` 使用领域动作名，如 `relay.ws.handle_message`、`agent_run.launch_commit`、`session.launch.prepare_runtime`。
   - `stage` 使用操作内阶段名，如 `deserialize`、`backend_send`、`db_query`、`spawn_process`、`stream_decode`。
4. 上下文字段只放事实，不重复拼接 message：
   - 标准关联字段：`session_id`、`run_id`、`backend_id`。
   - 路径特定字段：`workflow_id`、`project_id`、`client_command_id`、`request_id`、`event_kind`、`attempt`、`retry_count`。
5. 涉及用户输入、命令、环境和输出内容时只记录安全摘要：
   - 可以记录布尔状态、长度、枚举、计数、已脱敏路径。
   - 不记录 token、secret、完整命令行、env 原文、模型输出正文、完整 stdout/stderr。

## 推荐分片

并行派发时按不重叠写入范围拆分：

1. Relay / WebSocket
   - `crates/agentdash-relay/**`
   - `crates/agentdash-api/src/relay/**`
   - 重点：连接注册、消息解析、发送失败、后端断连。
2. AgentRun / Session Launch
   - `crates/agentdash-application-agentrun/**`
   - `crates/agentdash-api/src/routes/session/**`
   - 重点：运行启动、fork、commit、runtime/session repository、工作区 materialization。
3. Local Runtime / Stream
   - `crates/agentdash-local/**`
   - `crates/agentdash-local-tauri/**`
   - `crates/agentdash-stream/**`
   - 重点：本地后端连接、进程/pty/stream decode、前后台桥接。
4. Infra / API / VFS
   - `crates/agentdash-infrastructure/**`
   - `crates/agentdash-api/src/routes/**`
   - `crates/agentdash-application-vfs/**`
   - 重点：DB/runtime、HTTP route 边界、VFS 失败入口。

每个分片完成后先跑对应 crate 的 `cargo check`，最后由 check agent 跑 fmt、clippy 和全局审计 grep。

## 非目标

- 不改变 diagnostics 存储层、查询端点、ring buffer、JSON 文件输出策略。
- 不新增私有子系统或内部集成字段。
- 不把领域事件、session event、context audit、shell 输出流迁入 diagnostics。
