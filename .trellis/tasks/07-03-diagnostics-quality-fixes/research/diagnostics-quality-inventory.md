# diagnostics 质量审计摘要

## 扫描结论

主仓已经有 diagnostics facade 和 clippy 守门，裸 `tracing::{info,warn,error,debug,trace}!` 在公开 workspace 中基本不是主要问题。主要问题集中在部分 `diag!(Warn/Error)` 调用点仍然使用手写错误消息或缺少结构化上下文，导致查询 diagnostics 时只能看到一句话，无法稳定定位 operation / stage / error debug / 关联 id。

建议修复口径：

- 有错误对象的 Warn/Error 调用优先使用 `diag_error!`。
- 无错误对象但属于关键失败告警的调用补齐 `operation`、`stage` 和上下文字段。
- 普通生命周期、状态提示和 debug 级别诊断继续使用 `diag!`。

## 高优先级候选

### AgentRun / 执行链路

- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
  - 存在仅输出 `{error}` 的 `diag!(Error, Subsystem::AgentRun, "{error}")` 风格调用。
  - 建议升级为 `diag_error!`，补 `operation`、`stage`、`run_id` / `session_id` / connector 相关字段。
- `crates/agentdash-application-agentrun/**`
  - 运行启动、fork、launch commit、frame 处理等路径中的 Warn/Error 需要逐项确认是否有错误对象和关联 id。

### Relay / WebSocket / 本地后端通信

- `crates/agentdash-api/src/relay/ws_handler.rs`
  - Warn/Error 调用点数量高，覆盖连接注册、消息收发、后端状态变化。
  - 建议拆成独立分片，逐个失败阶段补 `operation` / `stage` / `backend_id` / message kind。
- `crates/agentdash-relay/**`
  - 关注消息序列化、发送失败、连接断开和 backend registry 操作。

### Local Runtime / Stream

- `crates/agentdash-local/**`
- `crates/agentdash-local-tauri/**`
- `crates/agentdash-stream/**`
  - 重点关注本地进程、pty、WebSocket client、stream decode、后台任务失败。
  - 避免记录完整命令、环境变量、stdout/stderr 或模型输出正文。

### Infra / API / VFS

- `crates/agentdash-infrastructure/src/postgres_runtime.rs`
  - 例如“复用连接失败: {e}”这类调用应升级为 `diag_error!`，补 DB operation/stage。
- `crates/agentdash-api/src/routes/**`
  - API 边界失败应补 route operation 和 request 级上下文，但不改变 HTTP 响应语义。
- `crates/agentdash-application-vfs/**`
  - VFS 已有部分补齐，后续检查是否还有 route/service 层漏掉的错误对象。

## 参考扫描命令

```powershell
rg -n "diag!\\((Warn|Error)" crates
rg -n "diag_error!" crates
rg -n "DiagnosticErrorContext" crates
rg -n "tracing::(info|warn|error|debug|trace)!" crates
```

## 分片建议

1. Relay / WebSocket：先修连接和消息边界，风险高、上下文清晰。
2. AgentRun / Session Launch：修用户最容易感知的运行失败链路。
3. Local Runtime / Stream：修本地后端和流处理失败，注意脱敏。
4. Infra / API / VFS：收尾通用基础设施和 HTTP route 边界。

每个分片都应由独立 agent 处理不重叠文件范围，最后统一 check。
